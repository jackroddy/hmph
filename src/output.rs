//! The tables, written through toil.

use std::fs::File;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use toil::{Align, Cell, Column, Schema, Stream, Table};

use crate::sample::Exit;
use crate::summary::Summary;
use crate::timeline::Row;

/// The settings every table carries in its `#=` lines.
pub struct About {
    pub interval: Duration,
    pub threshold: f64,
    /// The command hmph spawned, or `None` when it attached.
    pub command: Option<String>,
}

impl About {
    fn write(&self, table: &mut Table) {
        table
            .meta("interval(s)", [self.interval.as_secs_f64()])
            .meta("threshold", [self.threshold])
            .meta("command", [self.command.clone()]);
    }
}

/// `ticks.tbl`, written a block per interval while the tree runs.
pub struct Ticks {
    stream: Stream<File>,
    t0: Instant,
}

impl Ticks {
    pub fn create(dir: &Path, t0: Instant, about: &About) -> io::Result<Ticks> {
        let schema = Schema::new([
            Column::new("t(s)")
                .fixed(3)
                .align(Align::Right)
                .min_width(9),
            Column::new("pid").align(Align::Right).min_width(7),
            Column::new("name").min_width(15),
            Column::new("cpus")
                .fixed(2)
                .align(Align::Right)
                .min_width(6),
            Column::new("threads").align(Align::Right),
            Column::new("rss(MiB)")
                .fixed(1)
                .align(Align::Right)
                .min_width(9),
        ]);
        let widths = schema.widths();
        let mut stream = Stream::new(schema, widths, File::create(dir.join("ticks.tbl"))?);
        stream.meta("interval(s)", [about.interval.as_secs_f64()])?;
        stream.meta("threshold", [about.threshold])?;
        stream.meta("command", [about.command.clone()])?;
        stream.header()?;
        Ok(Ticks { stream, t0 })
    }

    pub fn block(&mut self, rows: &[Row]) -> io::Result<()> {
        for row in rows {
            self.stream.row([
                Cell::from(seconds(row.at, self.t0)),
                row.pid.into(),
                row.name.as_str().into(),
                row.concurrency().into(),
                row.threads.into(),
                mebibytes(row.rss).into(),
            ])?;
        }
        Ok(())
    }
}

/// Write `summary.tbl` per process, `tree.tbl` per subtree, and `levels.tbl`.
pub fn write_summary(
    dir: &Path,
    t0: Instant,
    about: &About,
    exit: &Exit,
    own: &[Summary],
    tree: &[Summary],
) -> io::Result<()> {
    // the root is the first process of every snapshot, so
    // its subtree is the whole run
    let accounted = tree.first().map(|s| s.cpu.as_secs_f64());
    for (file, summaries) in [("summary.tbl", own), ("tree.tbl", tree)] {
        let mut table = Table::new(summary_schema());
        about.write(&mut table);
        table
            .meta("status", [exit.status.clone()])
            .meta("rusage_cpu(s)", [exit.rusage.map(|d| d.as_secs_f64())])
            .meta("accounted_cpu(s)", [accounted]);
        for s in summaries {
            table.row([
                Cell::from(s.pid),
                s.ppid.into(),
                s.name.as_str().into(),
                seconds(s.start, t0).into(),
                seconds(s.end, t0).into(),
                s.wall.as_secs_f64().into(),
                s.cpu.as_secs_f64().into(),
                s.charged.as_secs_f64().into(),
                s.serial.as_secs_f64().into(),
                s.parallel.as_secs_f64().into(),
                s.mean.into(),
                s.max.into(),
                s.argv.as_str().into(),
            ]);
        }
        table.write(dir.join(file))?;
    }

    let mut table = Table::new([
        Column::new("scope"),
        Column::new("pid").align(Align::Right),
        Column::new("name"),
        Column::new("cpus").align(Align::Right),
        Column::new("wall(s)").fixed(3).align(Align::Right),
    ]);
    about.write(&mut table);
    for (scope, summaries) in [("process", own), ("tree", tree)] {
        for s in summaries {
            for (level, wall) in s.levels.iter().enumerate() {
                if wall.is_zero() {
                    continue;
                }
                table.row([
                    Cell::from(scope),
                    s.pid.into(),
                    s.name.as_str().into(),
                    level.into(),
                    wall.as_secs_f64().into(),
                ]);
            }
        }
    }
    table.write(dir.join("levels.tbl"))
}

fn summary_schema() -> Schema {
    let seconds = |label| Column::new(label).fixed(3).align(Align::Right);
    let cpus = |label| Column::new(label).fixed(2).align(Align::Right);
    Schema::new([
        Column::new("pid").align(Align::Right),
        Column::new("ppid").align(Align::Right),
        Column::new("name"),
        seconds("start(s)"),
        seconds("end(s)"),
        seconds("wall(s)"),
        seconds("cpu(s)"),
        seconds("charged(s)"),
        seconds("serial(s)"),
        seconds("parallel(s)"),
        cpus("mean"),
        cpus("max"),
        Column::new("argv").ragged(),
    ])
}

/// Seconds from `t0` to `at`, negative when `at` came first.
fn seconds(at: Instant, t0: Instant) -> f64 {
    if at >= t0 {
        (at - t0).as_secs_f64()
    } else {
        -(t0 - at).as_secs_f64()
    }
}

fn mebibytes(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
