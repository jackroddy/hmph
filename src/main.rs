//! hmph: how much parallelization happens.
//!
//! Watches a process tree and writes a timeline of each process's concurrency,
//! cpu time consumed over wall elapsed, tick by tick.

mod output;
mod probe;
mod sample;
mod summary;
mod timeline;

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use output::{About, Ticks};
use probe::Snapshot;
use timeline::Row;

#[derive(Parser)]
#[command(name = "hmph", about = "how much parallelization happens", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Spawn a command and watch it until it exits.
    Run(RunArgs),
    /// Watch a process that is already running.
    Attach(AttachArgs),
}

#[derive(Args)]
struct Common {
    /// Where the tables go
    #[arg(long, value_name = "dir", default_value = ".")]
    out: PathBuf,

    /// Milliseconds between samples
    #[arg(long, default_value_t = 100)]
    interval: u64,
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    common: Common,

    /// The command to run
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

#[derive(Args)]
struct AttachArgs {
    #[command(flatten)]
    common: Common,

    /// The process to watch
    pid: u32,
}

/// Turns each snapshot into rows, streams them, and keeps them for the summary.
struct Watch {
    about: About,
    out: PathBuf,
    prev: Option<Snapshot>,
    t0: Option<Instant>,
    ticks: Option<Ticks>,
    rows: Vec<Row>,
    /// One row per interval for the whole tree.
    all: Vec<Row>,
}

impl Watch {
    fn tick(&mut self, snapshot: Snapshot) -> Result<()> {
        let Some(prev) = self.prev.take() else {
            self.ticks = Some(Ticks::create(&self.out, snapshot.at, &self.about)?);
            self.t0 = Some(snapshot.at);
            self.prev = Some(snapshot);
            return Ok(());
        };
        let rows = timeline::rows(&prev, &snapshot);
        let all = summary::whole_run(&rows);
        self.ticks.as_mut().unwrap().block(&all, &rows)?;
        self.rows.extend(rows);
        self.all.extend(all);
        self.prev = Some(snapshot);
        Ok(())
    }
}

fn main() -> Result<()> {
    let (common, command, pid) = match Cli::parse().command {
        Command::Run(args) => (args.common, Some(args.command), None),
        Command::Attach(args) => (args.common, None, Some(args.pid)),
    };
    let interval = Duration::from_millis(common.interval);
    std::fs::create_dir_all(&common.out)
        .with_context(|| format!("cannot create {}", common.out.display()))?;
    let mut watch = Watch {
        about: About {
            interval,
            command: command.as_ref().map(|c| c.join(" ")),
        },
        out: common.out,
        prev: None,
        t0: None,
        ticks: None,
        rows: Vec::new(),
        all: Vec::new(),
    };

    let exit = match (command, pid) {
        (Some(command), _) => sample::run(&command, interval, |s| watch.tick(s))?,
        (_, Some(pid)) => sample::attach(pid, interval, |s| watch.tick(s))?,
        (None, None) => unreachable!(),
    };

    let t0 = watch.t0.expect("at least one snapshot");
    let own = summary::per_process(&watch.rows);
    let run = summary::per_process(&watch.all)
        .pop()
        .context("the tree was never seen")?;
    output::write_tables(&watch.out, t0, &watch.about, &exit, &run, &own)?;

    let mut err = std::io::stderr().lock();
    output::screen(&mut err, &watch.about, &run, &own, &watch.all)?;
    if let Some(rusage) = exit.rusage {
        let seen = run.cpu.as_secs_f64() / rusage.as_secs_f64().max(f64::EPSILON);
        if seen < 0.9 {
            writeln!(
                err,
                "hmph: saw {:.0}% of the cpu the run used; the rest went to programs that \
                 lived and died between samples, so try a shorter --interval",
                100.0 * seen
            )?;
        }
    }
    std::process::exit(exit.code.unwrap_or(0))
}
