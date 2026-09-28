# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `hmph run -- <command>` spawns a command and samples the process tree under
  it until it exits, then exits with the command's own status.
- `hmph attach <pid>` samples a tree that is already running until its root is
  gone.
- `ticks.tbl`: one row per process per interval, streamed while the tree runs,
  with concurrency in cpus, live threads and resident size.
- `summary.tbl`: one row per process with its wall, cpu, serial and parallel
  time, mean and peak concurrency, and argv.
- `tree.tbl`: the same figures for each process's subtree, so the root's row is
  the whole run.
- `levels.tbl`: wall time at each integer level of concurrency, per process and
  per subtree.
- `--interval` (ms, default 100), `--threshold` (cpus, default 1.2) and `--out`
  (directory, default `.`), recorded in each table's `#=` lines beside the
  command, the exit status, and the cpu the kernel charged the tree against the
  cpu the timeline accounted for.
