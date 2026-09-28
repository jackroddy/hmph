//! The screen and the tables, written through toil.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use toil::{Align, Cell, Column, Schema, Stream, Table};

use crate::sample::Exit;
use crate::summary::Summary;
use crate::timeline::Row;

/// The settings every table carries in its `#=` lines.
pub struct About {
    pub interval: Duration,
    /// The command hmph spawned, or `None` when it attached.
    pub command: Option<String>,
}

impl About {
    fn write(&self, table: &mut Table) {
        table
            .meta("command", [self.command.clone()])
            .meta("interval(s)", [self.interval.as_secs_f64()]);
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
            Column::new("program").min_width(16),
            Column::new("cpus")
                .fixed(2)
                .align(Align::Right)
                .min_width(6),
            Column::new("threads").align(Align::Right),
            Column::new("rss(MiB)")
                .fixed(1)
                .align(Align::Right)
                .min_width(9),
            Column::new("pid").align(Align::Right).min_width(7),
        ]);
        let widths = schema.widths();
        let mut stream = Stream::new(schema, widths, File::create(dir.join("ticks.tbl"))?);
        stream.meta("command", [about.command.clone()])?;
        stream.meta("interval(s)", [about.interval.as_secs_f64()])?;
        stream.header()?;
        Ok(Ticks { stream, t0 })
    }

    /// One block: the whole run's row, then each process's own.
    pub fn block(&mut self, all: &[Row], rows: &[Row]) -> io::Result<()> {
        for row in all.iter().chain(rows) {
            self.stream.row([
                Cell::from(seconds(row.at, self.t0)),
                label(row).into(),
                row.concurrency().into(),
                row.threads.into(),
                mebibytes(row.rss).into(),
                pid(row.pid),
            ])?;
        }
        Ok(())
    }
}

/// Write `summary.tbl` and `cpus.tbl`.
pub fn write_tables(
    dir: &Path,
    t0: Instant,
    about: &About,
    exit: &Exit,
    run: &Summary,
    own: &[Summary],
) -> io::Result<()> {
    let mut table = Table::new(summary_schema());
    about.write(&mut table);
    table
        .meta("status", [exit.status.clone()])
        .meta("wall(s)", [run.wall.as_secs_f64()])
        .meta("cpu(s)", [run.cpu.as_secs_f64()])
        .meta("cpus", [run.mean]);
    for s in own {
        table.row(summary_row(s, t0));
    }
    table.write(dir.join("summary.tbl"))?;

    let mut table = Table::new(cpus_schema());
    about.write(&mut table);
    for (level, share, time) in levels(run) {
        table.row([Cell::from(level), time.into(), share.into()]);
    }
    table.write(dir.join("cpus.tbl"))
}

/// Print the screen: the run as a whole, then what ran when.
pub fn screen(
    out: &mut impl Write,
    about: &About,
    run: &Summary,
    own: &[Summary],
    all: &[Row],
) -> io::Result<()> {
    if let Some(command) = &about.command {
        writeln!(out, "hmph: {command}")?;
    }
    writeln!(
        out,
        "wall {:.1} s   cpu {:.1} s   cpus {:.1} average, {:.1} peak",
        run.wall.as_secs_f64(),
        run.cpu.as_secs_f64(),
        run.mean,
        run.max
    )?;
    writeln!(
        out,
        "{}  cpus over time, 0 to {:.1}",
        sparkline(all, run.start, 60),
        run.max
    )?;
    writeln!(out)?;

    let mut table = Table::new(cpus_schema());
    for (level, share, time) in levels(run) {
        table.row([Cell::from(level), time.into(), share.into()]);
    }
    out.write_all(table.render().as_bytes())?;
    writeln!(out)?;

    let mut table = Table::new(screen_schema());
    for s in own {
        table.row(summary_row(s, run.start));
    }
    out.write_all(table.render().as_bytes())?;
    if own.len() > 1 {
        writeln!(
            out,
            "cpus is each program's own work, so one waiting on the programs under it reads low"
        )?;
    }
    Ok(())
}

/// A sparkline of the whole run's concurrency, `width` columns wide.
fn sparkline(all: &[Row], t0: Instant, width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let Some(last) = all.last() else {
        return String::new();
    };
    let span = seconds(last.at, t0).max(f64::EPSILON);
    let peak = all.iter().map(Row::concurrency).fold(0.0, f64::max);
    let mut rows = all.iter().peekable();
    (0..width)
        .map(|i| {
            // each column reads the interval that covers its
            // midpoint, so a run shorter than the width still
            // fills every column
            let t = (i as f64 + 0.5) / width as f64 * span;
            while let Some(row) = rows.peek()
                && seconds(row.at, t0) < t
            {
                rows.next();
            }
            let Some(row) = rows.peek() else {
                return ' ';
            };
            let c = row.concurrency();
            if peak == 0.0 || c == 0.0 {
                return ' ';
            }
            let bar = ((c / peak) * BARS.len() as f64).ceil() as usize;
            BARS[bar.clamp(1, BARS.len()) - 1]
        })
        .collect()
}

/// The run's wall at each number of cpus, most time first.
fn levels(run: &Summary) -> Vec<(usize, f64, f64)> {
    let total = run.wall.as_secs_f64().max(f64::EPSILON);
    let mut levels: Vec<(usize, f64, f64)> = run
        .levels
        .iter()
        .enumerate()
        .filter(|(_, wall)| !wall.is_zero())
        .map(|(level, wall)| {
            let seconds = wall.as_secs_f64();
            (level, 100.0 * seconds / total, seconds)
        })
        .collect();
    levels.sort_by(|a, b| b.2.total_cmp(&a.2));
    levels
}

fn summary_row(s: &Summary, t0: Instant) -> [Cell; 8] {
    [
        Cell::from(label_of(&s.name, &s.argv)),
        seconds(s.start, t0).into(),
        s.wall.as_secs_f64().into(),
        s.cpu.as_secs_f64().into(),
        s.mean.into(),
        s.max.into(),
        pid(s.pid),
        s.argv.as_str().into(),
    ]
}

fn screen_schema() -> Schema {
    Schema::new([
        Column::new("program"),
        Column::new("start(s)").fixed(1).align(Align::Right),
        Column::new("time(s)").fixed(1).align(Align::Right),
        Column::new("cpu(s)").fixed(1).align(Align::Right),
        Column::new("cpus").fixed(1).align(Align::Right),
        Column::new("peak").fixed(1).align(Align::Right),
    ])
}

fn summary_schema() -> Schema {
    Schema::new([
        Column::new("program"),
        Column::new("start(s)").fixed(3).align(Align::Right),
        Column::new("time(s)").fixed(3).align(Align::Right),
        Column::new("cpu(s)").fixed(3).align(Align::Right),
        Column::new("cpus").fixed(2).align(Align::Right),
        Column::new("peak").fixed(2).align(Align::Right),
        Column::new("pid").align(Align::Right),
        Column::new("argv").ragged(),
    ])
}

fn cpus_schema() -> Schema {
    Schema::new([
        Column::new("cpus").align(Align::Right),
        Column::new("time(s)").fixed(1).align(Align::Right),
        Column::new("share(%)").fixed(0).align(Align::Right),
    ])
}

/// The program and its subcommand, if the first argument reads as one.
fn label(row: &Row) -> String {
    label_of(&row.name, &row.argv)
}

fn label_of(name: &str, argv: &str) -> String {
    let mut words = argv.split_whitespace();
    let _program = words.next();
    match words.next() {
        Some(word)
            if word.starts_with(|c: char| c.is_ascii_alphabetic())
                && !word.contains('/')
                && word.len() <= 16 =>
        {
            format!("{name} {word}")
        }
        _ => name.to_owned(),
    }
}

/// The whole run's rows carry pid 0, which no process has.
fn pid(pid: u32) -> Cell {
    if pid == 0 {
        Cell::missing()
    } else {
        pid.into()
    }
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
