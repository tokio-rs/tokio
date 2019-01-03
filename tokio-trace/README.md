# tokio-trace

A scoped, structured logging and diagnostics system.

## Overview

`tokio-trace` is a framework for instrumenting Rust programs to collect
structured, event-based diagnostic information.

In asynchronous systems like Tokio, interpreting traditional log messages can
often be quite challenging. Since individual tasks are multiplexed on the same
thread, associated events and log lines are intermixed making it difficult to
trace the logic flow. `tokio-trace` expands upon logging-style diagnostics by
allowing libraries and applications to record structured events with additional
information about *temporality* and *causality* --- unlike a log message, a span
in `tokio-trace` has a beginning and end time, may be entered and exited by the
flow of execution, and may exist within a nested tree of similar spans. In
addition, `tokio-trace` spans are *structured*, with the ability to record typed
data as well as textual messages.

The `tokio-trace` crate provides the APIs necessary for instrumenting libraries
and applications to emit trace data.

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-trace = { git = "https://github.com/tokio-rs/tokio" }
```

Next, add this to your crate:

```rust
#[macro_use]
extern crate tokio_trace;
```

This crate provides macros for creating `Span`s and `Event`s, which represent
periods of time and momentary events within the execution of a program,
respectively.

Users of the `log` crate should note that `tokio-trace` exposes a set of macros for
creating `Event`s (`trace!`, `debug!`, `info!`, `warn!`, and `error!`) which may
be invoked with the same syntax as the similarly-named macros from the `log`
crate. Often, the process of converting a project to use `tokio-trace` can begin
with a simple drop-in replacement.

You can find examples showing how to use this crate in the examples directory.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
