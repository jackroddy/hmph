//! hmph: how much parallelization happens.
//!
//! Watches a process tree and writes a timeline of each process's concurrency,
//! cpu time consumed over wall elapsed, tick by tick.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hmph", about = "how much parallelization happens")]
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

#[derive(Parser)]
struct RunArgs {
    /// Where the tables go
    #[arg(long, value_name = "dir")]
    out: Option<std::path::PathBuf>,

    /// Milliseconds between samples
    #[arg(long, default_value_t = 100)]
    interval: u64,

    /// The command to run
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

#[derive(Parser)]
struct AttachArgs {
    /// Where the tables go
    #[arg(long, value_name = "dir")]
    out: Option<std::path::PathBuf>,

    /// Milliseconds between samples
    #[arg(long, default_value_t = 100)]
    interval: u64,

    /// The process to watch
    pid: u32,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Run(_) => anyhow::bail!("run is not written yet"),
        Command::Attach(_) => anyhow::bail!("attach is not written yet"),
    }
}
