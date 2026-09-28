//! Consecutive snapshots to one row per process per interval.

use std::time::{Duration, Instant};

use crate::probe::{Process, Snapshot};

/// What one process did over one interval between two snapshots.
#[derive(Debug, Clone)]
pub struct Row {
    /// When the interval ended.
    pub at: Instant,
    /// How much of the interval the process was alive for.
    pub wall: Duration,
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub argv: String,
    /// Cpu time consumed over the interval.
    pub cpu: Duration,
    /// Cpu time the kernel has charged the process so far, gone threads included.
    pub charged: Duration,
    pub threads: usize,
    pub rss: u64,
}

impl Row {
    /// Compute the concurrency: cpu time consumed over wall elapsed, in cpus.
    pub fn concurrency(&self) -> f64 {
        if self.wall.is_zero() {
            0.0
        } else {
            self.cpu.as_secs_f64() / self.wall.as_secs_f64()
        }
    }
}

/// One row for every process in `next`.
///
/// A process absent from `prev` is credited everything it has consumed so far,
/// over the wall since it started. A process absent from `next` gets no row:
/// its last interval is lost, and the summary reports the gap against rusage.
pub fn rows(prev: &Snapshot, next: &Snapshot) -> Vec<Row> {
    next.processes
        .iter()
        .map(|process| match prev.get(process.pid) {
            Some(before) => Row {
                at: next.at,
                wall: next.at.saturating_duration_since(prev.at),
                cpu: cpu_since(before, process),
                ..first(process, next.at)
            },
            None => {
                let from = process.started.max(prev.at);
                Row {
                    wall: next.at.saturating_duration_since(from),
                    ..first(process, next.at)
                }
            }
        })
        .collect()
}

/// The row for a process that was not in the earlier snapshot.
fn first(process: &Process, at: Instant) -> Row {
    Row {
        at,
        wall: Duration::ZERO,
        pid: process.pid,
        ppid: process.ppid,
        name: process.name.clone(),
        argv: process.argv.join(" "),
        cpu: process.threads.iter().map(|t| t.cpu).sum(),
        charged: process.cpu,
        threads: process.threads.len(),
        rss: process.rss,
    }
}

/// Cpu time the live threads consumed between two readings of one process.
fn cpu_since(before: &Process, after: &Process) -> Duration {
    after
        .threads
        .iter()
        .map(|thread| {
            // a thread not seen before is credited everything it
            // has consumed, since it began inside this interval;
            // a thread gone since is lost, as a process would be
            let earlier = before
                .threads
                .iter()
                .find(|t| t.tid == thread.tid)
                .map_or(Duration::ZERO, |t| t.cpu);
            thread.cpu.saturating_sub(earlier)
        })
        .sum()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::probe::Thread;

    pub(crate) fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A process whose threads have consumed the given milliseconds each.
    pub(crate) fn process(
        pid: u32,
        ppid: u32,
        started: Instant,
        threads: &[(u32, u64)],
    ) -> Process {
        Process {
            pid,
            ppid,
            name: format!("p{pid}"),
            argv: vec![],
            started,
            cpu: threads.iter().map(|&(_, c)| ms(c)).sum(),
            threads: threads
                .iter()
                .map(|&(tid, c)| Thread { tid, cpu: ms(c) })
                .collect(),
            rss: 4096,
        }
    }

    pub(crate) fn snapshot(at: Instant, processes: Vec<Process>) -> Snapshot {
        Snapshot { at, processes }
    }

    #[test]
    fn delta_over_the_interval() {
        let t0 = Instant::now();
        let prev = snapshot(t0, vec![process(1, 0, t0, &[(1, 10), (2, 50)])]);
        let next = snapshot(t0 + ms(100), vec![process(1, 0, t0, &[(1, 60), (2, 150)])]);
        let rows = rows(&prev, &next);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].wall, ms(100));
        assert_eq!(rows[0].cpu, ms(150));
        assert!((rows[0].concurrency() - 1.5).abs() < 1e-9);
        assert_eq!(rows[0].threads, 2);
    }

    #[test]
    fn new_process_credited_since_it_started() {
        let t0 = Instant::now();
        let prev = snapshot(t0, vec![]);
        // began 40 ms into a 100 ms interval, ran flat out
        let next = snapshot(t0 + ms(100), vec![process(2, 1, t0 + ms(60), &[(2, 40)])]);
        let rows = rows(&prev, &next);
        assert_eq!(rows[0].wall, ms(40));
        assert!((rows[0].concurrency() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn new_process_older_than_the_interval_is_clipped_to_it() {
        let t0 = Instant::now();
        let prev = snapshot(t0 + ms(100), vec![]);
        let next = snapshot(t0 + ms(200), vec![process(2, 1, t0, &[(2, 100)])]);
        let rows = rows(&prev, &next);
        assert_eq!(rows[0].wall, ms(100));
    }

    #[test]
    fn new_thread_credited_and_gone_thread_lost() {
        let t0 = Instant::now();
        let prev = snapshot(t0, vec![process(1, 0, t0, &[(1, 10), (2, 500)])]);
        let next = snapshot(t0 + ms(100), vec![process(1, 0, t0, &[(1, 20), (3, 30)])]);
        let rows = rows(&prev, &next);
        assert_eq!(rows[0].cpu, ms(40));
    }

    #[test]
    fn gone_process_has_no_row() {
        let t0 = Instant::now();
        let prev = snapshot(
            t0,
            vec![process(1, 0, t0, &[(1, 0)]), process(2, 1, t0, &[(2, 0)])],
        );
        let next = snapshot(t0 + ms(100), vec![process(1, 0, t0, &[(1, 0)])]);
        let rows = rows(&prev, &next);
        assert_eq!(rows.iter().map(|r| r.pid).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn zero_wall_is_zero_concurrency() {
        let t0 = Instant::now();
        let prev = snapshot(t0, vec![]);
        let next = snapshot(t0, vec![process(1, 0, t0, &[(1, 5)])]);
        assert_eq!(rows(&prev, &next)[0].concurrency(), 0.0);
    }
}
