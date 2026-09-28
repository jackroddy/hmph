//! One snapshot of the process tree under a root pid.
//!
//! Only this module reads the OS. Everything above it takes a [`Snapshot`]
//! and never asks where it came from.

use std::time::{Duration, Instant};

/// One process at the instant a snapshot was taken.
#[derive(Debug, Clone)]
pub struct Process {
    pub pid: u32,
    /// The kernel's short name for the process.
    pub name: String,
    pub argv: Vec<String>,
    /// When the process began, on the snapshot's clock.
    pub started: Instant,
    /// Cpu time each live thread has consumed.
    pub threads: Vec<Thread>,
    /// Resident set size in bytes.
    pub rss: u64,
}

/// One thread's cpu time at the instant a snapshot was taken.
#[derive(Debug, Clone)]
pub struct Thread {
    pub tid: u32,
    pub cpu: Duration,
}

/// Every process under a root at one instant, the root first.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// When the snapshot was taken.
    pub at: Instant,
    pub processes: Vec<Process>,
}

impl Snapshot {
    pub fn get(&self, pid: u32) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid == pid)
    }
}

#[cfg(target_os = "linux")]
pub use linux::snapshot;

#[cfg(target_os = "linux")]
mod linux {
    use super::{Process, Snapshot, Thread};
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    /// Read the tree under `root`. An empty snapshot means the root is gone.
    pub fn snapshot(root: u32) -> io::Result<Snapshot> {
        let at = Instant::now();
        let uptime = uptime()?;
        let mut processes = Vec::new();
        let mut pending = vec![root];
        while let Some(pid) = pending.pop() {
            // a process that exited between the directory listing
            // and its files is not an error, it is just gone
            let Some((process, children)) = read_process(pid, at, uptime)? else {
                continue;
            };
            processes.push(process);
            pending.extend(children);
        }
        Ok(Snapshot { at, processes })
    }

    /// One process and the pids its threads have forked, or `None` if it is gone.
    fn read_process(
        pid: u32,
        at: Instant,
        uptime: Duration,
    ) -> io::Result<Option<(Process, Vec<u32>)>> {
        let dir = PathBuf::from(format!("/proc/{pid}"));
        let Some(stat) = read_if_present(&dir.join("stat"))? else {
            return Ok(None);
        };
        let Some(stat) = parse_stat(&stat) else {
            return Ok(None);
        };
        let argv = read_if_present(&dir.join("cmdline"))?
            .map(|text| {
                text.split('\0')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        let mut threads = Vec::new();
        let mut children = Vec::new();
        let Ok(tasks) = fs::read_dir(dir.join("task")) else {
            return Ok(None);
        };
        for task in tasks {
            let task = task?.path();
            let Some(tid) = task.file_name().and_then(|n| n.to_str()?.parse().ok()) else {
                continue;
            };
            if let Some(text) = read_if_present(&task.join("schedstat"))?
                && let Some(cpu) = parse_schedstat(&text)
            {
                threads.push(Thread { tid, cpu });
            }
            if let Some(text) = read_if_present(&task.join("children"))? {
                children.extend(
                    text.split_whitespace()
                        .filter_map(|s| s.parse::<u32>().ok()),
                );
            }
        }

        let age = uptime.saturating_sub(ticks(stat.start_ticks));
        let process = Process {
            pid,
            name: stat.comm,
            argv,
            started: at - age,
            threads,
            rss: stat.rss_pages * page_size(),
        };
        Ok(Some((process, children)))
    }

    struct Stat {
        comm: String,
        start_ticks: u64,
        rss_pages: u64,
    }

    /// The fields of `/proc/<pid>/stat` this tool reads.
    fn parse_stat(text: &str) -> Option<Stat> {
        // comm is in parentheses and may itself contain
        // spaces and parentheses, so split at the last one
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        let comm = text[open + 1..close].to_owned();
        // proc(5) numbers fields from 1 with pid first and
        // comm second, so the field after comm is field 3
        let field: Vec<&str> = text[close + 1..].split_whitespace().collect();
        let at = |n: usize| field.get(n - 3);
        Some(Stat {
            comm,
            start_ticks: at(22)?.parse().ok()?,
            rss_pages: at(24)?.parse().ok()?,
        })
    }

    /// Nanoseconds on cpu, the first field of `schedstat`.
    fn parse_schedstat(text: &str) -> Option<Duration> {
        text.split_whitespace()
            .next()?
            .parse()
            .ok()
            .map(Duration::from_nanos)
    }

    fn uptime() -> io::Result<Duration> {
        let text = fs::read_to_string("/proc/uptime")?;
        let seconds: f64 = text
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| io::Error::other("cannot parse /proc/uptime"))?;
        Ok(Duration::from_secs_f64(seconds))
    }

    /// `Ok(None)` when the file is gone, which a dead process's files are.
    fn read_if_present(path: &Path) -> io::Result<Option<String>> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) =>
            {
                Ok(None)
            }
            // a task that exits mid-read answers ESRCH
            Err(e) if e.raw_os_error() == Some(libc::ESRCH) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn ticks(n: u64) -> Duration {
        Duration::from_secs_f64(n as f64 / clock_ticks_per_second())
    }

    fn clock_ticks_per_second() -> f64 {
        // SAFETY: sysconf takes an integer and touches no memory
        unsafe { libc::sysconf(libc::_SC_CLK_TCK) as f64 }
    }

    fn page_size() -> u64 {
        // SAFETY: sysconf takes an integer and touches no memory
        unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stat_with_spaces_and_parens_in_comm() {
            let line = "12 (a (b) c) S 7 12 12 0 -1 4194304 88 0 0 0 30 40 0 0 20 0 3 0 1813161 3313664 436 0";
            let s = parse_stat(line).unwrap();
            assert_eq!(s.comm, "a (b) c");
            assert_eq!(s.start_ticks, 1813161);
            assert_eq!(s.rss_pages, 436);
        }

        #[test]
        fn snapshot_of_self_has_self_first() {
            let snap = snapshot(std::process::id()).unwrap();
            assert_eq!(snap.processes[0].pid, std::process::id());
            assert!(!snap.processes[0].threads.is_empty());
        }
    }
}
