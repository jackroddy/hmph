//! The timeline integrated per process and per subtree.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::timeline::Row;

/// One process, or one subtree, integrated over its rows.
#[derive(Debug, Clone)]
pub struct Summary {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub argv: String,
    /// When the first row began.
    pub start: Instant,
    /// When the last row ended.
    pub end: Instant,
    /// Wall time alive, the rows' walls summed.
    pub wall: Duration,
    /// Cpu time consumed, as the timeline saw it.
    pub cpu: Duration,
    /// Cpu time the kernel had charged by the last row, gone threads included.
    pub charged: Duration,
    /// Wall time at concurrency at or under the threshold.
    pub serial: Duration,
    /// Cpu time consumed while over the threshold.
    pub parallel: Duration,
    /// Cpu over wall.
    pub mean: f64,
    /// The highest concurrency of any row.
    pub max: f64,
    /// Wall time at each integer level of concurrency, indexed by level.
    pub levels: Vec<Duration>,
}

/// Integrate every process's own rows. Rows come in timeline order.
pub fn per_process(rows: &[Row], threshold: f64) -> Vec<Summary> {
    let mut order = Vec::new();
    let mut summaries: HashMap<u32, Summary> = HashMap::new();
    for row in rows {
        let summary = summaries.entry(row.pid).or_insert_with(|| {
            order.push(row.pid);
            Summary {
                pid: row.pid,
                ppid: row.ppid,
                name: row.name.clone(),
                argv: row.argv.clone(),
                start: row.at - row.wall,
                end: row.at,
                wall: Duration::ZERO,
                cpu: Duration::ZERO,
                charged: Duration::ZERO,
                serial: Duration::ZERO,
                parallel: Duration::ZERO,
                mean: 0.0,
                max: 0.0,
                levels: Vec::new(),
            }
        });
        // a process that was reparented reports its last parent
        summary.ppid = row.ppid;
        summary.end = row.at;
        summary.wall += row.wall;
        summary.cpu += row.cpu;
        summary.charged = row.charged;
        let c = row.concurrency();
        if c <= threshold {
            summary.serial += row.wall;
        } else {
            summary.parallel += row.cpu;
        }
        summary.max = summary.max.max(c);
        let level = c.round() as usize;
        if summary.levels.len() <= level {
            summary.levels.resize(level + 1, Duration::ZERO);
        }
        summary.levels[level] += row.wall;
    }
    order
        .into_iter()
        .map(|pid| {
            let mut s = summaries.remove(&pid).unwrap();
            s.mean = if s.wall.is_zero() {
                0.0
            } else {
                s.cpu.as_secs_f64() / s.wall.as_secs_f64()
            };
            s
        })
        .collect()
}

/// Integrate every process's subtree: each row's cpu is added to the row of
/// every ancestor in the same interval, then the rows are integrated as usual.
pub fn per_subtree(rows: &[Row], threshold: f64) -> Vec<Summary> {
    let own = per_process(rows, threshold);
    let mut tree = per_process(&subtree_rows(rows), threshold);
    // what the kernel charged a subtree is the sum over its
    // members' final figures, since members die at different
    // times and a row only carries the ones still alive
    let parent: HashMap<u32, u32> = own.iter().map(|s| (s.pid, s.ppid)).collect();
    for summary in &mut tree {
        summary.charged = own
            .iter()
            .filter(|member| is_under(&parent, member.pid, summary.pid))
            .map(|member| member.charged)
            .sum();
    }
    tree
}

/// Whether `pid` is `root` or descends from it.
fn is_under(parent: &HashMap<u32, u32>, mut pid: u32, root: u32) -> bool {
    loop {
        if pid == root {
            return true;
        }
        match parent.get(&pid) {
            Some(&next) => pid = next,
            None => return false,
        }
    }
}

/// Rows with each process's cpu summed over its subtree, interval by interval.
fn subtree_rows(rows: &[Row]) -> Vec<Row> {
    let mut parent: HashMap<u32, u32> = HashMap::new();
    let mut out: Vec<Row> = Vec::new();
    // rows of one interval share an `at`, and arrive together
    let mut interval_start = 0;
    for (i, row) in rows.iter().enumerate() {
        parent.insert(row.pid, row.ppid);
        if row.at != rows[interval_start].at {
            interval_start = i;
        }
        out.push(row.clone());
        // add this row's cpu to every ancestor with a row
        // in this interval; an ancestor without one has
        // already gone and the cpu stays with the child
        let mut pid = row.ppid;
        while let Some(&next) = parent.get(&pid) {
            if let Some(ancestor) = out[interval_start..i].iter_mut().find(|r| r.pid == pid) {
                ancestor.cpu += row.cpu;
                ancestor.threads += row.threads;
                ancestor.rss += row.rss;
            }
            pid = next;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::tests::ms;

    fn row(at: Instant, wall: u64, pid: u32, ppid: u32, cpu: u64) -> Row {
        Row {
            at,
            wall: ms(wall),
            pid,
            ppid,
            name: format!("p{pid}"),
            argv: String::new(),
            cpu: ms(cpu),
            charged: ms(cpu),
            threads: 1,
            rss: 0,
        }
    }

    #[test]
    fn serial_and_parallel_split_at_the_threshold() {
        let t0 = Instant::now();
        let rows = vec![
            row(t0 + ms(100), 100, 1, 0, 100),
            row(t0 + ms(200), 100, 1, 0, 400),
            row(t0 + ms(300), 100, 1, 0, 40),
        ];
        let s = &per_process(&rows, 1.2)[0];
        assert_eq!(s.wall, ms(300));
        assert_eq!(s.cpu, ms(540));
        assert_eq!(s.serial, ms(200));
        assert_eq!(s.parallel, ms(400));
        assert!((s.max - 4.0).abs() < 1e-9);
        assert!((s.mean - 540.0 / 300.0).abs() < 1e-9);
        assert_eq!(s.start, t0);
        assert_eq!(s.end, t0 + ms(300));
        assert_eq!(s.levels, [ms(100), ms(100), ms(0), ms(0), ms(100)]);
    }

    #[test]
    fn subtree_sums_children_into_ancestors_per_interval() {
        let t0 = Instant::now();
        let t1 = t0 + ms(100);
        let t2 = t0 + ms(200);
        let rows = vec![
            row(t1, 100, 1, 0, 100),
            row(t1, 100, 2, 1, 100),
            row(t1, 100, 3, 2, 200),
            row(t2, 100, 1, 0, 0),
            row(t2, 100, 3, 2, 300),
        ];
        let tree = per_subtree(&rows, 1.2);
        let root = tree.iter().find(|s| s.pid == 1).unwrap();
        assert_eq!(root.cpu, ms(700));
        assert_eq!(root.wall, ms(200));
        assert!((root.max - 4.0).abs() < 1e-9);
        // 2 was gone in the second interval, so 3's cpu then stays with 3
        let mid = tree.iter().find(|s| s.pid == 2).unwrap();
        assert_eq!(mid.cpu, ms(300));
        // charged is each member's last row, so 0 + 100 + 300
        assert_eq!(root.charged, ms(400));
        assert_eq!(mid.charged, ms(400));
        let own = per_process(&rows, 1.2);
        assert_eq!(own.iter().find(|s| s.pid == 1).unwrap().cpu, ms(100));
    }
}
