## Keeping track of issues and PRs

The Tokio GitHub repository has a lot of issues and PRs to keep track of. This
section explains the meaning of various labels, as well as our [GitHub
project][project]. The section is primarily targeted at maintainers.  Most
contributors aren't able to set these labels.

### Area

The area label describes the crates relevant to this issue or PR.

- **A-ci** This issue concerns our GitHub Actions setup.
- **A-tokio** This issue concerns the main Tokio crate.
- **A-readme** This issue is related to documentation such as README.md.
- **A-benches** This issue concerns the benchmarks.
- **A-examples** This issue concerns the examples.
- **A-tokio-test** The issue concerns the `tokio-test` crate.
- **A-tokio-util** This issue concerns the `tokio-util` crate.
- **A-tokio-macros** This issue concerns the `tokio-macros` crate. Should only
  be used for the procedural macros, and not `join!` or `select!`.
- **A-tokio-stream** This issue concerns the `tokio-stream` crate.

### Category

- **C-bug** This is a bug-report. Bug-fix PRs use `C-enhancement` instead.
- **C-enhancement** This is a PR that adds a new features.
- **C-maintenance** This is an issue or PR about stuff such as documentation,
  GitHub Actions or code quality.
- **C-feature-request** This is a feature request. Implementations of feature
  requests use `C-enhancement` instead.
- **C-feature-accepted** If you submit a PR for this feature request, we won't
  close it with the reason "we don't want this". Issues with this label should
  also have the `C-feature-request` label.
- **C-musing** Stuff like tracking issues or roadmaps. "musings about a better
  world"
- **C-proposal** A proposal of some kind, and a request for comments.
- **C-question** A user question. Large overlap with GitHub discussions.
- **C-request** A non-feature request, e.g. "please add deprecation notices to
  `-alpha.*` versions of crates"

### Calls for participation

- **E-help-wanted** Stuff where we want help. Often seen together with `C-bug`
  or `C-feature-accepted`.
- **E-easy** This is easy, ranging from quick documentation fixes to stuff you
  can do after reading the tutorial on our website.
- **E-medium** This is not `E-easy` or `E-hard`.
- **E-hard** This either involves very tricky code, is something we don't know
  how to solve, or is challenging for some other reason.
- **E-needs-mvce** This bug is missing a minimal complete and verifiable
  example.

The "E-" prefix is the same as used in the Rust compiler repository. Some
issues are missing a difficulty rating, but feel free to ask on our Discord
server if you want to know how challenging an issue likely is.

### Module

The module label provides a more fine grained categorization than **Area**.

- **M-blocking** Things relevant to `spawn_blocking`, `block_in_place`.
- **M-codec** The `tokio_util::codec` module.
- **M-compat** The `tokio_util::compat` module.
- **M-coop** Things relevant to coop.
- **M-fs** The `tokio::fs` module.
- **M-io** The `tokio::io` module.
- **M-macros** Issues about any kind of macro.
- **M-metrics** Things relevant to `tokio::runtime::metrics`.
- **M-net** The `tokio::net` module.
- **M-process** The `tokio::process` module.
- **M-runtime** The `tokio::runtime` module.
- **M-signal** The `tokio::signal` module.
- **M-sync** The `tokio::sync` module.
- **M-task** The `tokio::task` module.
- **M-time** The `tokio::time` module.
- **M-tracing** Tracing support in Tokio.
- **M-taskdump** Things relevant to taskdump.

### Topic

Some extra information.

- **T-docs** This is about documentation.
- **T-performance** This is about performance.
- **T-v0.1.x** This is about old Tokio.

Any label not listed here is not in active use.

[project]: https://github.com/orgs/tokio-rs/projects/1
