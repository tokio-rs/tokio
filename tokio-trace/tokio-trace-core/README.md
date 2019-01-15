# tokio-trace-core

Core primitives for `tokio-trace`.

## Overview

`tokio-trace` is a framework for instrumenting Rust programs to collect
structured, event-based diagnostic information. This crate defines the core
primitives of `tokio-trace`.

The crate provides:

* `Span` identifies a span within the execution of a program.

* `Subscriber`, the trait implemented to collect trace data.

* `Metadata` and `Callsite` provide information describing `Span`s.

* `Field` and `FieldSet` describe and access the structured data attached to a
  `Span`.

* `Dispatch` allows span events to be dispatched to `Subscriber`s.

In addition, it defines the global callsite registry and per-thread current
dispatcher which other components of the tracing system rely on.

Application authors will typically not use this crate directly. Instead, they
will use the [`tokio-trace`] crate, which provides a much more fully-featured
API. However, this crate's API will change very infrequently, so it may be used
when dependencies must be very stable.

[`tokio-trace`]: ../

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
