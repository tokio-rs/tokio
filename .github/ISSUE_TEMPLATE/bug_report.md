---
name: Bug report
about: Create a report to help us improve
title: ''
labels: A-tokio, C-bug
assignees: ''

---

**Version**
List the versions of all `tokio` crates you are using. The easiest way to get
this information is using `cargo-tree`.

`cargo install cargo-tree`
(see install here: https://github.com/sfackler/cargo-tree)

Then:

`cargo tree | grep tokio`

**Platform**
The output of `uname -a` (UNIX), or version and 32 or 64-bit (Windows)

**Description**
Enter your issue details here.
One way to structure the description:

[short summary of the bug]

I tried this code:

[code sample that causes the bug]

I expected to see this happen: [explanation]

Instead, this happened: [explanation]
