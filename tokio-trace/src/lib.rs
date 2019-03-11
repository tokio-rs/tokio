//! A scoped, structured logging and diagnostics system.
//!
//! # Overview
//!
//! `tokio-trace` is a framework for instrumenting Rust programs to collect
//! structured, event-based diagnostic information.
//!
//! In asynchronous systems like Tokio, interpreting traditional log messages can
//! often be quite challenging. Since individual tasks are multiplexed on the same
//! thread, associated events and log lines are intermixed making it difficult to
//! trace the logic flow. `tokio-trace` expands upon logging-style diagnostics by
//! allowing libraries and applications to record structured events with additional
//! information about *temporality* and *causality* — unlike a log message, a span
//! in `tokio-trace` has a beginning and end time, may be entered and exited by the
//! flow of execution, and may exist within a nested tree of similar spans. In
//! addition, `tokio-trace` spans are *structured*, with the ability to record typed
//! data as well as textual messages.
//!
//! The `tokio-trace` crate provides the APIs necessary for instrumenting libraries
//! and applications to emit trace data.
//!
//! # Core Concepts
//!
//! The core of `tokio-trace`'s API is composed of `Event`s, `Span`s, and
//! `Subscriber`s. We'll cover these in turn.
//!
//! ## `Span`s
//!
//! A [`Span`] represents a _period of time_ during which a program was executing
//! in some context. A thread of execution is said to _enter_ a span when it
//! begins executing in that context, and to _exit_ the span when switching to
//! another context. The span in which a thread is currently executing is
//! referred to as the _current_ span.
//!
//! Spans form a tree structure — unless it is a root span, all spans have a
//! _parent_, and may have one or more _children_. When a new span is created,
//! the current span becomes the new span's parent. The total execution time of
//! a span consists of the time spent in that span and in the entire subtree
//! represented by its children. Thus, a parent span always lasts for at least
//! as long as the longest-executing span in its subtree.
//!
//! In addition, data may be associated with spans. A span may have _fields_ —
//! a set of key-value pairs describing the state of the program during that
//! span; an optional name, and metadata describing the source code location
//! where the span was originally entered.
//!
//! ### When to use spans
//!
//! As a rule of thumb, spans should be used to represent discrete units of work
//! (e.g., a given request's lifetime in a server) or periods of time spent in a
//! given context (e.g., time spent interacting with an instance of an external
//! system, such as a database).
//!
//! Which scopes in a program correspond to new spans depend somewhat on user
//! intent. For example, consider the case of a loop in a program. Should we
//! construct one span and perform the entire loop inside of that span, like:
//! ```rust
//! # #[macro_use] extern crate tokio_trace;
//! # fn main() {
//! # let n = 1;
//! span!("my loop").enter(|| {
//!     for i in 0..n {
//!         # let _ = i;
//!         // ...
//!     }
//! })
//! # }
//! ```
//! Or, should we create a new span for each iteration of the loop, as in:
//! ```rust
//! # #[macro_use] extern crate tokio_trace;
//! # fn main() {
//! # let n = 1u64;
//! for i in 0..n {
//!     # let _ = i;
//!     span!("my loop", iteration = i).enter(|| {
//!         // ...
//!     })
//! }
//! # }
//! ```
//!
//! Depending on the circumstances, we might want to do either, or both. For
//! example, if we want to know how long was spent in the loop overall, we would
//! create a single span around the entire loop; whereas if we wanted to know how
//! much time was spent in each individual iteration, we would enter a new span
//! on every iteration.
//!
//! ## Events
//!
//! An [`Event`] represents a _point_ in time. It signifies something that
//! happened while the trace was executing. `Event`s are comparable to the log
//! records emitted by unstructured logging code, but unlike a typical log line,
//! an `Event` may occur within the context of a `Span`. Like a `Span`, it
//! may have fields, and implicitly inherits any of the fields present on its
//! parent span, and it may be linked with one or more additional
//! spans that are not its parent; in this case, the event is said to _follow
//! from_ those spans.
//!
//! Essentially, `Event`s exist to bridge the gap between traditional
//! unstructured logging and span-based tracing. Similar to log records, they
//! may be recorded at a number of levels, and can have unstructured,
//! human-readable messages; however, they also carry key-value data and exist
//! within the context of the tree of spans that comprise a trace. Thus,
//! individual log record-like events can be pinpointed not only in time, but
//! in the logical execution flow of the system.
//!
//! Events are represented as a special case of spans — they are created, they
//! may have fields added, and then they close immediately, without being
//! entered.
//!
//! In general, events should be used to represent points in time _within_ a
//! span — a request returned with a given status code, _n_ new items were
//! taken from a queue, and so on.
//!
//! ## `Subscriber`s
//!
//! As `Span`s and `Event`s occur, they are recorded or aggregated by
//! implementations of the [`Subscriber`] trait. `Subscriber`s are notified
//! when an `Event` takes place and when a `Span` is entered or exited. These
//! notifications are represented by the following `Subscriber` trait methods:
//! + [`observe_event`], called when an `Event` takes place,
//! + [`enter`], called when execution enters a `Span`,
//! + [`exit`], called when execution exits a `Span`
//!
//! In addition, subscribers may implement the [`enabled`] function to _filter_
//! the notifications they receive based on [metadata] describing each `Span`
//! or `Event`. If a call to `Subscriber::enabled` returns `false` for a given
//! set of metadata, that `Subscriber` will *not* be notified about the
//! corresponding `Span` or `Event`. For performance reasons, if no currently
//! active subscribers express  interest in a given set of metadata by returning
//! `true`, then the corresponding `Span` or `Event` will never be constructed.
//!
//! # Usage
//!
//! First, add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! tokio-trace = { git = "https://github.com/tokio-rs/tokio" }
//! ```
//!
//! Next, add this to your crate:
//!
//! ```rust
//! #[macro_use]
//! extern crate tokio_trace;
//! # fn main() {}
//! ```
//!
//! `Span`s are constructed using the `span!` macro, and then _entered_
//! to indicate that some code takes place within the context of that `Span`:
//!
//! ```rust
//! # #[macro_use]
//! # extern crate tokio_trace;
//! # fn main() {
//! // Construct a new span named "my span".
//! let mut span = span!("my span");
//! span.enter(|| {
//!     // Any trace events in this closure or code called by it will occur within
//!     // the span.
//! });
//! // Dropping the span will close it, indicating that it has ended.
//! # }
//! ```
//!
//! `Event`s are created using the `event!` macro, and are recorded when the
//! event is dropped:
//!
//! ```rust
//! # #[macro_use]
//! # extern crate tokio_trace;
//! # fn main() {
//! use tokio_trace::Level;
//! event!(Level::INFO, "something has happened!");
//! # }
//! ```
//!
//! Users of the [`log`] crate should note that `tokio-trace` exposes a set of
//! macros for creating `Event`s (`trace!`, `debug!`, `info!`, `warn!`, and
//! `error!`) which may be invoked with the same syntax as the similarly-named
//! macros from the `log` crate. Often, the process of converting a project to
//! use `tokio-trace` can begin with a simple drop-in replacement.
//!
//! Let's consider the `log` crate's yak-shaving example:
//!
//! ```rust
//! #[macro_use]
//! extern crate tokio_trace;
//! use tokio_trace::field;
//! # #[derive(Debug)] pub struct Yak(String);
//! # impl Yak { fn shave(&mut self, _: u32) {} }
//! # fn find_a_razor() -> Result<u32, u32> { Ok(1) }
//! # fn main() {
//! pub fn shave_the_yak(yak: &mut Yak) {
//!     // Create a new span for this invocation of `shave_the_yak`, annotated
//!     // with  the yak being shaved as a *field* on the span.
//!     span!("shave_the_yak", yak = field::debug(&yak)).enter(|| {
//!         // Since the span is annotated with the yak, it is part of the context
//!         // for everything happening inside the span. Therefore, we don't need
//!         // to add it to the message for this event, as the `log` crate does.
//!         info!(target: "yak_events", "Commencing yak shaving");
//!
//!         loop {
//!             match find_a_razor() {
//!                 Ok(razor) => {
//!                     // We can add the razor as a field rather than formatting it
//!                     // as part of the message, allowing subscribers to consume it
//!                     // in a more structured manner:
//!                     info!({ razor = field::display(razor) }, "Razor located");
//!                     yak.shave(razor);
//!                     break;
//!                 }
//!                 Err(err) => {
//!                     // However, we can also create events with formatted messages,
//!                     // just as we would for log records.
//!                     warn!("Unable to locate a razor: {}, retrying", err);
//!                 }
//!             }
//!         }
//!     })
//! }
//! # }
//! ```
//!
//! You can find examples showing how to use this crate in the examples
//! directory.
//!
//! ### In libraries
//!
//! Libraries should link only to the `tokio-trace` crate, and use the provided
//! macros to record whatever information will be useful to downstream
//! consumers.
//!
//! ### In executables
//!
//! In order to record trace events, executables have to use a `Subscriber`
//! implementation compatible with `tokio-trace`. A `Subscriber` implements a
//! way of collecting trace data, such as by logging it to standard output.
//!
//! Unlike the `log` crate, `tokio-trace` does *not* use a global `Subscriber`
//! which is initialized once. Instead, it follows the `tokio` pattern of
//! executing code in a context. For example:
//!
//! ```rust
//! #[macro_use]
//! extern crate tokio_trace;
//! # pub struct FooSubscriber;
//! # use tokio_trace::{span::{Id, Attributes, Record}, Metadata};
//! # impl tokio_trace::Subscriber for FooSubscriber {
//! #   fn new_span(&self, _: &Attributes) -> Id { Id::from_u64(0) }
//! #   fn record(&self, _: &Id, _: &Record) {}
//! #   fn event(&self, _: &tokio_trace::Event) {}
//! #   fn record_follows_from(&self, _: &Id, _: &Id) {}
//! #   fn enabled(&self, _: &Metadata) -> bool { false }
//! #   fn enter(&self, _: &Id) {}
//! #   fn exit(&self, _: &Id) {}
//! # }
//! # impl FooSubscriber {
//! #   fn new() -> Self { FooSubscriber }
//! # }
//! # fn main() {
//! let my_subscriber = FooSubscriber::new();
//!
//! tokio_trace::subscriber::with_default(my_subscriber, || {
//!     // Any trace events generated in this closure or by functions it calls
//!     // will be collected by `my_subscriber`.
//! })
//! # }
//! ```
//!
//! This approach allows trace data to be collected by multiple subscribers
//! within different contexts in the program. Alternatively, a single subscriber
//! may be constructed by the `main` function and all subsequent code executed
//! with that subscriber as the default. Any trace events generated outside the
//! context of a subscriber will not be collected.
//!
//! The executable itself may use the `tokio-trace` crate to instrument itself
//! as well.
//!
//! The [`tokio-trace-nursery`] repository contains less stable crates designed
//! to be used with the `tokio-trace` ecosystem. It includes a collection of
//! `Subscriber` implementations, as well as utility and adapter crates.
//!
//! In particular, the following `tokio-trace-nursery` crates are likely to be
//! of interest:
//!
//! - [`tokio-trace-futures`] provides a compatibility layer with the `futures`
//!   crate, allowing spans to be attached to `Future`s, `Stream`s, and `Executor`s.
//! - [`tokio-trace-fmt`] provides a `Subscriber` implementation for
//!   logging formatted trace data to stdout, with similar filtering and
//!   formatting to the `env-logger` crate.
//! - [`tokio-trace-log`] provides a compatibility layer with the `log` crate,
//!   allowing log `Record`s to be recorded as `tokio-trace` `Event`s within the
//!   trace tree. This is useful when a project using `tokio-trace` have
//!   dependencies which use `log`.
//!
//! [`log`]: https://docs.rs/log/0.4.6/log/
//! [`Span`]: span/struct.Span
//! [`Event`]: struct.Event.html
//! [`Subscriber`]: subscriber/trait.Subscriber.html
//! [`observe_event`]: subscriber/trait.Subscriber.html#tymethod.observe_event
//! [`enter`]: subscriber/trait.Subscriber.html#tymethod.enter
//! [`exit`]: subscriber/trait.Subscriber.html#tymethod.exit
//! [`enabled`]: subscriber/trait.Subscriber.html#tymethod.enabled
//! [metadata]: struct.Metadata.html
//! [`tokio-trace-nursery`]: https://github.com/tokio-rs/tokio-trace-nursery
//! [`tokio-trace-futures`]: https://github.com/tokio-rs/tokio-trace-nursery/tree/master/tokio-trace-futures
//! [`tokio-trace-fmt`]: https://github.com/tokio-rs/tokio-trace-nursery/tree/master/tokio-trace-fmt
//! [`tokio-trace-log`]: https://github.com/tokio-rs/tokio-trace-nursery/tree/master/tokio-trace-log
extern crate tokio_trace_core;

// Somehow this `use` statement is necessary for us to re-export the `core`
// macros on Rust 1.26.0. I'm not sure how this makes it work, but it does.
#[allow(unused_imports)]
#[doc(hidden)]
use tokio_trace_core::*;

pub use self::{
    dispatcher::Dispatch,
    event::Event,
    field::Value,
    span::Span,
    subscriber::Subscriber,
    tokio_trace_core::{dispatcher, event, Level, Metadata},
};

#[doc(hidden)]
pub use self::{
    span::Id,
    tokio_trace_core::{
        callsite::{self, Callsite},
        metadata,
    },
};

#[macro_use]
mod macros;

pub mod field;
pub mod span;
pub mod subscriber;

mod sealed {
    pub trait Sealed {}
}
