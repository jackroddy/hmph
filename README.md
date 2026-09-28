# hmph

> **<u>h</u>ow <u>m</u>uch <u>p</u>arallelization <u>h</u>appens**

## about
`hmph` watches a process and the processes under it, and writes a timeline of
how many cpus each one was using across its lifetime. From that it works out
how much of a run was serial, how much was parallel work, and how parallel it
got. It reads the Linux `/proc` tree and nothing else.
The tool is purpose-built for my own use cases, and it was primarily developed using Claude code.
