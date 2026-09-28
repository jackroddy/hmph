//! Spawn a command or attach to a pid, and snapshot its tree at a fixed
//! interval until it is gone.

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::probe::{self, Snapshot};

/// What is known about the root once it is gone.
#[derive(Debug, Clone, Default)]
pub struct Exit {
    /// How the root ended, `exit N` or `signal N`, when hmph spawned it.
    pub status: Option<String>,
    /// The code hmph should exit with: the root's own, or 128 plus its signal.
    pub code: Option<i32>,
    /// Cpu time the kernel charged the root and everything it waited for.
    pub rusage: Option<Duration>,
}

/// Spawn `command` and snapshot its tree every `interval` until it exits.
///
/// `tick` first sees an empty snapshot stamped at the spawn, so that the
/// timeline credits the root everything from its first instant.
pub fn run(
    command: &[String],
    interval: Duration,
    mut tick: impl FnMut(Snapshot) -> Result<()>,
) -> Result<Exit> {
    let (program, args) = command.split_first().context("no command given")?;
    let child = Command::new(program)
        .args(args)
        .spawn()
        .with_context(|| format!("cannot run {program}"))?;
    let pid = child.id();
    let t0 = Instant::now();
    // ^C goes to the whole foreground group, so the child
    // already gets it; ignoring it here lets the tables
    // still be written after the child dies of it
    // SAFETY: setting a disposition, no handler runs
    unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };

    tick(Snapshot {
        at: t0,
        processes: Vec::new(),
    })?;
    let mut exit = None;
    let mut n = 0u32;
    while exit.is_none() {
        // the snapshot comes before the reap so a root that
        // has just exited is still in /proc for it
        tick(probe::snapshot(pid)?)?;
        exit = reap(pid as libc::pid_t)?;
        n += 1;
        sleep_until(t0 + interval * n);
    }
    Ok(exit.unwrap())
}

/// Snapshot the tree under `pid` every `interval` until the pid is gone.
pub fn attach(
    pid: u32,
    interval: Duration,
    mut tick: impl FnMut(Snapshot) -> Result<()>,
) -> Result<Exit> {
    let t0 = Instant::now();
    let mut n = 0u32;
    loop {
        let snapshot = probe::snapshot(pid)?;
        let gone = snapshot.processes.is_empty();
        if gone && n == 0 {
            anyhow::bail!("no process {pid}");
        }
        tick(snapshot)?;
        if gone {
            return Ok(Exit::default());
        }
        n += 1;
        sleep_until(t0 + interval * n);
    }
}

/// Collect the child if it has exited, without waiting for it.
fn reap(pid: libc::pid_t) -> Result<Option<Exit>> {
    let mut status = 0;
    // SAFETY: rusage is a plain struct the kernel fills in whole
    let mut rusage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: both out-pointers are to live locals
    let done = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut rusage) };
    if done == 0 {
        return Ok(None);
    }
    if done < 0 {
        return Err(std::io::Error::last_os_error()).context("wait4");
    }
    let (status, code) = if libc::WIFEXITED(status) {
        let code = libc::WEXITSTATUS(status);
        (format!("exit {code}"), code)
    } else {
        let signal = libc::WTERMSIG(status);
        (format!("signal {signal}"), 128 + signal)
    };
    Ok(Some(Exit {
        status: Some(status),
        code: Some(code),
        rusage: Some(timeval(rusage.ru_utime) + timeval(rusage.ru_stime)),
    }))
}

fn timeval(t: libc::timeval) -> Duration {
    Duration::new(t.tv_sec as u64, (t.tv_usec * 1000) as u32)
}

fn sleep_until(deadline: Instant) {
    thread::sleep(deadline.saturating_duration_since(Instant::now()));
}
