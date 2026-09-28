# hmph

A Rust command-line tool that watches a process tree and writes a timeline of
how many cpus each process was using across its lifetime: cpu time consumed
over wall elapsed, tick by tick. From the timeline it works out how much of a
run was serial, how much was parallel work, and how parallel it got, per
process and per subtree.

Work and facts live in foam, not here: `foam ready` for what is open, `foam
memories` for what earlier sessions found. This file holds the rules.

## The measurement

Concurrency over an interval is the cpu time the process's threads consumed
divided by the wall that elapsed. That is an integral, exact for the interval
however threads came and went inside it. Counting threads in a running state
at the tick is not the measurement: it aliases against anything periodic.

Serial time is the seconds spent at concurrency near one, and parallel work is
the concurrency integrated over the rest. The threshold between them is a flag
with a default, never a constant in the code.

## Layers

The core never reads the OS, and only the probe does.

- `probe` reads one snapshot of the tree under a root pid: each process, its
  parent, its name, when it started, its cpu time and its threads' cpu time.
  One implementation per platform behind `cfg`.
- `sample` spawns a command or attaches to a pid, ticks at a fixed interval,
  and keeps every snapshot stamped with when it was taken.
- `timeline` turns consecutive snapshots into one row per process per
  interval.
- `summary` integrates the timeline per process and per subtree.
- `output` writes the tables through `toil`.

`timeline` and `summary` take snapshots and give tables, so they carry the
unit tests, and a port to another platform is one probe.

## Platforms

Linux. The probe reads `/proc/<pid>/task/<tid>/schedstat` for nanoseconds on
cpu per thread, `/proc/<pid>/stat` for the parent, the name, the start time and
the process total that includes threads already gone, and `/proc/<pid>/cmdline`
for the argv. Nothing else is assumed. A Darwin probe over libproc is the
intended second platform and does not exist.

## Branches

All working changes go on `dev`. Do not open a feature branch unless I ask for
one.

`main` carries releases and nothing else. Push to it when cutting a release,
and leave it alone the rest of the time.

## Releases

hmph follows [semantic versioning](https://semver.org/spec/v2.0.0.html). While
the crate is below 1.0, a breaking change takes the minor number.

Tag every release, annotated, as `vMAJOR.MINOR.PATCH`, on the commit that was
published.

## Formatting

`rustfmt` is the format. Run `cargo fmt` before committing.

## Changelog

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Anything that changes what someone running the tool sees goes under
`[Unreleased]` in the commit that changes it.

## Temporary files

Scratch goes in `tmp-claude/` at the root, one directory per task, and never
into git.
