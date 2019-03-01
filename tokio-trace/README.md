# tokio-trace

A scoped, structured logging and diagnostics system.

[Documentation](https://tokio-rs.github.io/tokio/doc/tokio_trace/)

## Overview

`tokio-trace` is a framework for instrumenting Rust programs to collect
structured, event-based diagnostic information.

In asynchronous systems like Tokio, interpreting traditional log messages can
often be quite challenging. Since individual tasks are multiplexed on the same
thread, associated events and log lines are intermixed making it difficult to
trace the logic flow. `tokio-trace` expands upon logging-style diagnostics by
allowing libraries and applications to record structured events with additional
information about *temporality* and *causality* — unlike a log message, a span
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

As a rule of thumb, _spans_ should be used to represent discrete units of work
(e.g., a given request's lifetime in a server) or periods of time spent in a
given context (e.g., time spent interacting with an instance of an external
system, such as a database). In contrast, _events_ should be used to represent
points in time within a span — a request returned with a given status code,
_n_ new items were taken from a queue, and so on.

`Span`s are constructed using the `span!` macro, and then _entered_
to indicate that some code takes place within the context of that `Span`:

```rust
// Construct a new span named "my span".
let mut span = span!("my span");
span.enter(|| {
    // Any trace events in this closure or code called by it will occur within
    // the span.
});
// Dropping the span will close it, indicating that it has ended.
```

The `Event` type represent an event that occurs instantaneously, and is
essentially a `Span` that cannot be entered. They are created using the `event!`
macro:

```rust
use tokio_trace::Level;
event!(Level::INFO, "something has happened!");
```

Users of the [`log`] crate should note that `tokio-trace` exposes a set of macros for
creating `Event`s (`trace!`, `debug!`, `info!`, `warn!`, and `error!`) which may
be invoked with the same syntax as the similarly-named macros from the `log`
crate. Often, the process of converting a project to use `tokio-trace` can begin
with a simple drop-in replacement.

Let's consider the `log` crate's yak-shaving example:

```rust
#[macro_use]
extern crate tokio_trace;
use tokio_trace::field;

pub fn shave_the_yak(yak: &mut Yak) {
    // Create a new span for this invocation of `shave_the_yak`, annotated
    // with  the yak being shaved as a *field* on the span.
    span!("shave_the_yak", yak = field::debug(&yak)).enter(|| {
        // Since the span is annotated with the yak, it is part of the context
        // for everything happening inside the span. Therefore, we don't need
        // to add it to the message for this event, as the `log` crate does.
        info!(target: "yak_events", "Commencing yak shaving");

        loop {
            match find_a_razor() {
                Ok(razor) => {
                    // We can add the razor as a field rather than formatting it
                    // as part of the message, allowing subscribers to consume it
                    // in a more structured manner:
                    info!({ razor = field::display(razor) }, "Razor located");
                    yak.shave(razor);
                    break;
                }
                Err(err) => {
                    // However, we can also create events with formatted messages,
                    // just as we would for log records.
                    warn!("Unable to locate a razor: {}, retrying", err);
                }
            }
        }
    })
}
```

You can find examples showing how to use this crate in the examples directory.

### In libraries

Libraries should link only to the `tokio-trace` crate, and use the provided
macros to record whatever information will be useful to downstream consumers.

### In executables

In order to record trace events, executables have to use a `Subscriber`
implementation compatible with `tokio-trace`. A `Subscriber` implements a way of
collecting trace data, such as by logging it to standard output.

Unlike the `log` crate, `tokio-trace` does *not* use a global `Subscriber` which
is initialized once. Instead, it follows the `tokio` pattern of executing code
in a context. For example:

```rust
#[macro_use]
extern crate tokio_trace;

let my_subscriber = FooSubscriber::new();

tokio_trace::subscriber::with_default(subscriber, || {
    // Any trace events generated in this closure or by functions it calls
    // will be collected by `my_subscriber`.
})
```

This approach allows trace data to be collected by multiple subscribers within
different contexts in the program. Alternatively, a single subscriber may be
constructed by the `main` function and all subsequent code executed with that
subscriber as the default. Any trace events generated outside the context of a
subscriber will not be collected.

The executable itself may use the `tokio-trace` crate to instrument itself as
well.

The [`tokio-trace-nursery`] repository contains less stable crates designed to
be used with the `tokio-trace` ecosystem. It includes a collection of
`Subscriber` implementations, as well as utility and adapter crates.

[`log`]: https://docs.rs/log/0.4.6/log/
[`tokio-trace-nursery`]: https://github.com/tokio-rs/tokio-trace-nursery

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
