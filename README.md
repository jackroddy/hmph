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
exits, prints a screen on stderr, and exits with the command's own status.
`attach` watches a tree that is already running until its root is gone.

| flag         | default | what it is                   |
|--------------|---------|------------------------------|
| `--out`      | `.`     | directory the tables go in   |
| `--interval` | `100`   | milliseconds between samples |

## the screen

```text
hmph: nail search -t 4 pfam.hmm mgy-100k.fa --tbl-out nail.tbl
wall 138.8 s   cpu 529.2 s   cpus 3.8 average, 4.0 peak
▂▂████████████████████████████████████████▂▂████████████████  cpus over time, 0 to 4.0

# cpus time(s) share(%)
# ---- ------- --------
     4   130.3       94
     1     7.4        5
     2     0.4        0
     3     0.4        0
     0     0.3        0

# program        start(s) time(s) cpu(s) cpus peak
# -------------- -------- ------- ------ ---- ----
nail search           0.0   138.8  154.8  1.1  4.0
mmseqs prefilter      4.0    77.2  307.8  4.0  4.0
mmseqs align         81.4     8.6   34.4  4.0  4.0
mmseqs align         90.2     6.3   25.3  4.0  4.0
cpus is each program's own work, so one waiting on the programs under it reads low
```

The first two lines are the run as a whole. The first table is how its wall
time split by the number of cpus in use, rounded to the nearest whole cpu. The
second is each program that ran, when it started, how long it lived, and how
many cpus it used on average and at its peak. `cpus` there is the program's
own threads, so a parent that spends its life waiting on children reads low
while the children read high.

## the tables

Each table starts with `#=` lines carrying the command and the interval, then
a `#` header. `summary.tbl` also carries the exit status and the whole run's
wall, cpu and cpus.

`ticks.tbl` is one row per program per interval, with an `all` row first for
the whole run. hmph writes it a block per interval while the tree runs, so it
can be tailed or plotted:

```text
# t(s)    program          cpus   threads rss(MiB)  pid
# ------- ---------------- ------ ------- --------- -------
    0.500 all                3.59       7      24.7       -
    0.500 work.sh            0.00       1       3.4  201549
    0.500 xz                 3.54       5      19.5  201564
    0.500 head               0.05       1       1.8  201563
```

`summary.tbl` is the second table of the screen with three decimals, plus the
pid and the full argv, so a stage tree or a ledger can join on either.
`cpus.tbl` is the first table of the screen.

## the measurement

Concurrency over an interval is the cpu time a program's threads consumed
divided by the wall that elapsed. That is an integral over the interval, so a
thread that ran for part of it counts for that part. Counting threads in a
running state at the tick is not the measurement, since it aliases against
anything periodic.

hmph credits a program it sees for the first time everything the program has
consumed so far, over the wall since it started. A program gone from the next
sample loses its last interval, and so does a thread. hmph checks the whole
run's cpu against what the kernel charged the tree, and warns when it saw less
than nine tenths of it: programs lived and died between samples, and a shorter
`--interval` would catch them.

## platforms

Linux. The probe reads `/proc/<pid>/task/<tid>/schedstat` for nanoseconds on
cpu per thread, `/proc/<pid>/stat` for the parent, the name, the start time and
the process total, `/proc/<pid>/cmdline` for the argv, and
`/proc/<pid>/task/<tid>/children` to walk down the tree.
