# hmph

> **<u>h</u>ow <u>m</u>uch <u>p</u>arallelization <u>h</u>appens**

## about
`hmph` watches a process and the processes under it, and writes a timeline of
how many cpus each one was using across its lifetime. From that it works out
how much of a run was serial, how much was parallel work, and how parallel it
got. It reads the Linux `/proc` tree and nothing else.
The tool is purpose-built for my own use cases, and it was primarily developed using Claude code.

## usage

```sh
hmph run --out results -- make -j8
hmph attach --out results 12345
```

`run` spawns the command, samples the tree under it every 100 ms until it
exits, and exits with the command's own status. `attach` watches a tree that is
already running until its root is gone. It has no exact start and no `rusage`
to reconcile against, and those `#=` lines hold `-`.

| flag          | default | what it is                                          |
|---------------|---------|-----------------------------------------------------|
| `--out`       | `.`     | directory the tables go in                          |
| `--interval`  | `100`   | milliseconds between samples                        |
| `--threshold` | `1.2`   | concurrency at or under which an interval is serial |

## the measurement

Concurrency over an interval is the cpu time a process's threads consumed
divided by the wall that elapsed. That is an integral over the interval, so a
thread that ran for part of it counts for that part. Counting threads in a
running state at the tick is not the measurement, since it aliases against
anything periodic.

Serial time is wall time spent at concurrency at or under the threshold, and
parallel work is the cpu time consumed over the rest.

hmph credits a process it sees for the first time everything the process has
consumed so far, over the wall since it started. A process gone from the next
sample loses its last interval, and so does a thread. Two figures show what
that cost. `charged(s)` in the summary is what the kernel had charged the
process by the last sample, gone threads included. The `#=` lines put the cpu
`wait4` charged the whole tree beside the cpu the timeline accounted for.

## the tables

Each table starts with `#=` lines carrying the interval, the threshold and the
command, then a `#` header. The summary tables also carry the exit status,
`rusage_cpu(s)` and `accounted_cpu(s)`.

`ticks.tbl` is long, one row per process per interval, and is written a block
per interval while the tree runs, so it can be tailed:

```text
# t(s)    pid     name            cpus   threads rss(MiB)
# ------- ------- --------------- ------ ------- ---------
    0.500  201549 work.sh           0.00       1       3.4
    0.500  201564 xz                3.54       5      19.5
    0.500  201563 head              0.05       1       1.8
```

`summary.tbl` is one row per process. `tree.tbl` has the same columns, with
every figure taken over the process's subtree, so the root's row is the whole
run.

```text
# pid  ppid   name    start(s) end(s) wall(s) cpu(s) charged(s) serial(s) parallel(s) mean max  argv
# ---- ------ ------- -------- ------ ------- ------ ---------- --------- ----------- ---- ---- ----
201551 201550 sh         0.000  0.400   0.400  0.398      0.390     0.400       0.000 1.00 1.00 sh -c while :; do :; done
201564 201549 xz         0.400  1.000   0.600  2.348      2.340     0.000       2.348 3.91 3.99 xz -T4 -0
```

`levels.tbl` is the histogram: wall time at each integer level of concurrency,
one row per level per process (`scope process`) and per subtree (`scope tree`).

## platforms

Linux. The probe reads `/proc/<pid>/task/<tid>/schedstat` for nanoseconds on
cpu per thread, `/proc/<pid>/stat` for the parent, the name, the start time and
the process total, `/proc/<pid>/cmdline` for the argv, and
`/proc/<pid>/task/<tid>/children` to walk down the tree.
