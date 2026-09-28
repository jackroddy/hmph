//! The timeline integrated: the run as a whole, and each process's own share.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::timeline::Row;

/// One process, or the whole run, integrated over its rows.
#[derive(Debug, Clone)]
pub struct Summary {
    pub pid: u32,
    pub name: String,
    pub argv: String,
    /// When the first row began.
    pub start: Instant,
    /// When the last row ended.
    pub end: Instant,
    /// Wall time alive, the rows' walls summed.
    pub wall: Duration,
    /// Cpu time consumed.
    pub cpu: Duration,
    /// Cpu over wall.
    pub mean: f64,
    /// The highest concurrency of any row.
    pub max: f64,
    /// Wall time at each integer number of cpus, indexed by that number.
    pub levels: Vec<Duration>,
}

/// Integrate every process's own rows, in order of first appearance.
pub fn per_process(rows: &[Row]) -> Vec<Summary> {
    let mut order = Vec::new();
    let mut summaries: HashMap<u32, Summary> = HashMap::new();
    for row in rows {
        let summary = summaries.entry(row.pid).or_insert_with(|| {
            order.push(row.pid);
            Summary {
                pid: row.pid,
                name: row.name.clone(),
                argv: row.argv.clone(),
                start: row.at - row.wall,
                end: row.at,
                wall: Duration::ZERO,
                cpu: Duration::ZERO,
                mean: 0.0,
                max: 0.0,
                levels: Vec::new(),
            }
        });
        summary.end = row.at;
        summary.wall += row.wall;
        summary.cpu += row.cpu;
        let c = row.concurrency();
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

/// One row per interval for the whole tree: every process's cpu summed.
///
/// The rows of one interval share an `at` and arrive together, and the root
/// is alive for the whole of every interval, so its wall is the interval's.
pub fn whole_run(rows: &[Row]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for row in rows {
        match out.last_mut() {
            Some(total) if total.at == row.at => {
                total.cpu += row.cpu;
                total.threads += row.threads;
                total.rss += row.rss;
                total.wall = total.wall.max(row.wall);
            }
            _ => out.push(Row {
                pid: 0,
                name: "all".to_owned(),
                argv: String::new(),
                ..row.clone()
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::tests::ms;

    fn row(at: Instant, wall: u64, pid: u32, _ppid: u32, cpu: u64) -> Row {
        Row {
            at,
            wall: ms(wall),
            pid,
            name: format!("p{pid}"),
            argv: String::new(),
            cpu: ms(cpu),
            threads: 1,
            rss: 0,
        }
    }

    #[test]
    fn integrates_one_process() {
        let t0 = Instant::now();
        let rows = vec![
            row(t0 + ms(100), 100, 1, 0, 100),
            row(t0 + ms(200), 100, 1, 0, 400),
            row(t0 + ms(300), 100, 1, 0, 40),
        ];
        let s = &per_process(&rows)[0];
        assert_eq!(s.wall, ms(300));
        assert_eq!(s.cpu, ms(540));
        assert!((s.max - 4.0).abs() < 1e-9);
        assert!((s.mean - 540.0 / 300.0).abs() < 1e-9);
        assert_eq!(s.start, t0);
        assert_eq!(s.end, t0 + ms(300));
        assert_eq!(s.levels, [ms(100), ms(100), ms(0), ms(0), ms(100)]);
    }

    #[test]
    fn whole_run_sums_each_interval() {
        let t0 = Instant::now();
        let t1 = t0 + ms(100);
        let t2 = t0 + ms(200);
        let rows = vec![
            row(t1, 100, 1, 0, 100),
            row(t1, 100, 2, 1, 100),
            // began 40 ms before the tick
            row(t1, 40, 3, 2, 40),
            row(t2, 100, 1, 0, 0),
            row(t2, 100, 3, 2, 300),
        ];
        let all = whole_run(&rows);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].cpu, ms(240));
        assert_eq!(all[0].wall, ms(100));
        assert_eq!(all[0].threads, 3);
        assert_eq!(all[1].cpu, ms(300));
        let s = &per_process(&all)[0];
        assert_eq!(s.cpu, ms(540));
        assert_eq!(s.wall, ms(200));
        assert_eq!(s.name, "all");
    }
}
