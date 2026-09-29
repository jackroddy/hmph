# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-09-28

### Added

- `hmph run -- <command>` spawns a command and samples the process tree under
  it until it exits, then exits with the command's own status.
- `hmph attach <pid>` samples a tree that is already running until its root is
  gone.
- A screen on stderr when the run ends: wall, cpu and average cpus for the
  whole run, a sparkline of cpus over time, how the wall split by number of
  cpus, and which program ran when and how hard.
- `ticks.tbl`: one row per program per interval, plus an `all` row for the
  whole run, streamed while the tree runs.
- `summary.tbl`: one row per program with when it started, how long it ran,
  its cpu time, average and peak cpus, its pid and argv.
- `cpus.tbl`: the run's wall time and share at each number of cpus.
- `--interval` (ms, default 100) and `--out` (directory, default `.`).

[Unreleased]: https://github.com/jackroddy/hmph/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jackroddy/hmph/releases/tag/v0.1.0
