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
//! # use tokio_trace::{span::Id, Metadata, field::ValueSet};
//! # impl tokio_trace::Subscriber for FooSubscriber {
//! #   fn new_span(&self, _: &Metadata, _: &ValueSet) -> Id { Id::from_u64(0) }
//! #   fn record(&self, _: &Id, _: &ValueSet) {}
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

/// Constructs a new static callsite for a span or event.
#[doc(hidden)]
#[macro_export]
macro_rules! callsite {
    (name: $name:expr, fields: $( $field_name:expr ),* $(,)*) => ({
        callsite! {
            name: $name,
            target: module_path!(),
            level: $crate::Level::TRACE,
            fields: $( $field_name ),*
        }
    });
    (name: $name:expr, level: $lvl:expr, fields: $( $field_name:expr ),* $(,)*) => ({
        callsite! {
            name: $name,
            target: module_path!(),
            level: $lvl,
            fields: $( $field_name ),*
        }
    });
    (
        name: $name:expr,
        target: $target:expr,
        level: $lvl:expr,
        fields: $( $field_name:expr ),*
        $(,)*
    ) => ({
        use std::sync::{Once, atomic::{self, AtomicUsize, Ordering}};
        use $crate::{callsite, Metadata, subscriber::Interest};
        struct MyCallsite;
        static META: Metadata<'static> = {
            metadata! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: &[ $( stringify!($field_name) ),* ],
                callsite: &MyCallsite,
            }
        };
        // FIXME: Rust 1.34 deprecated ATOMIC_USIZE_INIT. When Tokio's minimum
        // supported version is 1.34, replace this with the const fn `::new`.
        #[allow(deprecated)]
        static INTEREST: AtomicUsize = atomic::ATOMIC_USIZE_INIT;
        static REGISTRATION: Once = Once::new();
        impl MyCallsite {
            #[inline]
            fn interest(&self) -> Interest {
                match INTEREST.load(Ordering::Relaxed) {
                    0 => Interest::never(),
                    2 => Interest::always(),
                    _ => Interest::sometimes(),
                }
            }
        }
        impl callsite::Callsite for MyCallsite {
            fn add_interest(&self, interest: Interest) {
                let current_interest = self.interest();
                let interest = match () {
                    // If the added interest is `never()`, don't change anything
                    // — either a different subscriber added a higher
                    // interest, which we want to preserve, or the interest is 0
                    // anyway (as it's initialized to 0).
                    _ if interest.is_never() => return,
                    // If the interest is `sometimes()`, that overwrites a `never()`
                    // interest, but doesn't downgrade an `always()` interest.
                    _ if interest.is_sometimes() && current_interest.is_never() => 1,
                    // If the interest is `always()`, we overwrite the current
                    // interest, as always() is the highest interest level and
                    // should take precedent.
                    _ if interest.is_always() => 2,
                    _ => return,
                };
                INTEREST.store(interest, Ordering::Relaxed);
            }
            fn clear_interest(&self) {
                INTEREST.store(0, Ordering::Relaxed);
            }
            fn metadata(&self) -> &Metadata {
                &META
            }
        }
        REGISTRATION.call_once(|| {
            callsite::register(&MyCallsite);
        });
        &MyCallsite
    })
}

/// Constructs a new span.
///
/// # Examples
///
/// Creating a new span with no fields:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let mut span = span!("my span");
/// span.enter(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
///
/// Creating a span with fields:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!("my span", foo = 2, bar = "a string").enter(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
///
/// Note that a trailing comma on the final field is valid:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!(
///     "my span",
///     foo = 2,
///     bar = "a string",
/// );
/// # }
/// ```
///
/// Creating a span with custom target and log level:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!(
///     target: "app_span",
///     level: tokio_trace::Level::TRACE,
///     "my span",
///     foo = 3,
///     bar = "another string"
/// );
/// # }
/// ```
///
/// Field values may be recorded after the span is created:
/// ```
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// let mut my_span = span!("my span", foo = 2, bar);
/// my_span.record("bar", &7);
/// # }
/// ```
///
/// Note that a span may have up to 32 fields. The following will not compile:
/// ```rust,compile_fail
///  # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// span!(
///     "too many fields!",
///     a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9,
///     j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16, q = 17,
///     r = 18, s = 19, t = 20, u = 21, v = 22, w = 23, x = 24, y = 25,
///     z = 26, aa = 27, bb = 28, cc = 29, dd = 30, ee = 31, ff = 32, gg = 33
/// );
/// # }
/// ```
#[macro_export]
macro_rules! span {
    (target: $target:expr, level: $lvl:expr, $name:expr, $($k:ident $( = $val:expr )* ),*,) => {
        span!(target: $target, level: $lvl, $name, $($k $( = $val)*),*)
    };
    (target: $target:expr, level: $lvl:expr, $name:expr, $($k:ident $( = $val:expr )* ),*) => {
        {
            use $crate::{callsite, field::{Value, ValueSet, AsField}, Span};
            use $crate::callsite::Callsite;
            let callsite = callsite! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: $($k),*
            };
            if is_enabled!(callsite) {
                let meta = callsite.metadata();
                Span::new(meta, &valueset!(meta.fields(), $($k $( = $val)*),*))
            } else {
                Span::new_disabled()
            }
        }
    };
    (target: $target:expr, level: $lvl:expr, $name:expr) => {
        span!(target: $target, level: $lvl, $name,)
    };
    (level: $lvl:expr, $name:expr, $($k:ident $( = $val:expr )* ),*,) => {
        span!(target: module_path!(), level: $lvl, $name, $($k $( = $val)*),*)
    };
    (level: $lvl:expr, $name:expr, $($k:ident $( = $val:expr )* ),*) => {
        span!(target: module_path!(), level: $lvl, $name, $($k $( = $val)*),*)
    };
    (level: $lvl:expr, $name:expr) => {
        span!(target: module_path!(), level: $lvl, $name,)
    };
    ($name:expr, $($k:ident $( = $val:expr)*),*,) => {
        span!(target: module_path!(), level: $crate::Level::TRACE, $name, $($k $( = $val)*),*)
    };
    ($name:expr, $($k:ident $( = $val:expr)*),*) => {
        span!(target: module_path!(), level: $crate::Level::TRACE, $name, $($k $( = $val)*),*)
    };
    ($name:expr) => { span!(target: module_path!(), level: $crate::Level::TRACE, $name,) };
}

/// Constructs a new `Event`.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// use tokio_trace::{Level, field};
///
/// # fn main() {
/// let data = (42, "fourty-two");
/// let private_data = "private";
/// let error = "a bad error";
///
/// event!(Level::ERROR, { error = field::display(error) }, "Received error");
/// event!(target: "app_events", Level::WARN, {
///         private_data = private_data,
///         data = field::debug(data),
///     },
///     "App warning: {}", error
/// );
/// event!(Level::INFO, the_answer = data.0);
/// # }
/// ```
///
/// Note that *unlike `span!`*, `event!` requires a value for all fields. As
/// events are recorded immediately when the macro is invoked, there is no
/// opportunity for fields to be recorded later. A trailing comma on the final
/// field is valid.
///
/// For example, the following does not compile:
/// ```rust,compile_fail
/// # #[macro_use]
/// # extern crate tokio_trace;
/// use tokio_trace::{Level, field};
///
/// # fn main() {
///     event!(Level::Info, foo = 5, bad_field, bar = field::display("hello"))
/// #}
/// ```
///
/// Events may have up to 32 fields. The following will not compile:
/// ```rust,compile_fail
///  # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// event!(tokio_trace::Level::INFO,
///     a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9,
///     j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16, q = 17,
///     r = 18, s = 19, t = 20, u = 21, v = 22, w = 23, x = 24, y = 25,
///     z = 26, aa = 27, bb = 28, cc = 29, dd = 30, ee = 31, ff = 32, gg = 33
/// );
/// # }
/// ```
#[macro_export]
macro_rules! event {
    (target: $target:expr, $lvl:expr, { $( $k:ident = $val:expr ),* $(,)*} )=> ({
        {
            #[allow(unused_imports)]
            use $crate::{callsite, dispatcher, Event, field::{Value, ValueSet}};
            use $crate::callsite::Callsite;
            let callsite = callsite! {
                name: concat!("event ", file!(), ":", line!()),
                target: $target,
                level: $lvl,
                fields: $( $k ),*
            };
            if is_enabled!(callsite) {
                let meta = callsite.metadata();
                Event::observe(meta, &valueset!(meta.fields(), $( $k = $val),* ));
            }
        }
    });
    (target: $target:expr, $lvl:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => ({
        event!(target: $target, $lvl, { message = format_args!($($arg)+), $( $k = $val ),* })
    });
    (target: $target:expr, $lvl:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => ({
        event!(target: $target, $lvl, { message = format_args!($($arg)+), $( $k = $val ),* })
    });
    (target: $target:expr, $lvl:expr, $( $k:ident = $val:expr ),+, ) => (
        event!(target: $target, $lvl, { $($k = $val),+ })
    );
    (target: $target:expr, $lvl:expr, $( $k:ident = $val:expr ),+ ) => (
        event!(target: $target, $lvl, { $($k = $val),+ })
    );
    (target: $target:expr, $lvl:expr, $($arg:tt)+ ) => (
        event!(target: $target, $lvl, { }, $($arg)+)
    );
    ( $lvl:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $lvl, { message = format_args!($($arg)+), $($k = $val),* })
    );
    ( $lvl:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $lvl, { message = format_args!($($arg)+), $($k = $val),* })
    );
    ( $lvl:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: module_path!(), $lvl, { $($k = $val),* })
    );
    ( $lvl:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: module_path!(), $lvl, { $($k = $val),* })
    );
    ( $lvl:expr, $($arg:tt)+ ) => (
        event!(target: module_path!(), $lvl, { }, $($arg)+)
    );
}

/// Constructs an event at the trace level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use std::time::SystemTime;
/// # #[derive(Debug, Copy, Clone)] struct Position { x: f32, y: f32 }
/// # impl Position {
/// # const ORIGIN: Self = Self { x: 0.0, y: 0.0 };
/// # fn dist(&self, other: Position) -> f32 {
/// #    let x = (other.x - self.x).exp2(); let y = (self.y - other.y).exp2();
/// #    (x + y).sqrt()
/// # }
/// # }
/// # fn main() {
/// use tokio_trace::field;
///
/// let pos = Position { x: 3.234, y: -1.223 };
/// let origin_dist = pos.dist(Position::ORIGIN);
///
/// trace!(position = field::debug(pos), origin_dist = field::debug(origin_dist));
/// trace!(target: "app_events",
///         { position = field::debug(pos) },
///         "x is {} and y is {}",
///        if pos.x >= 0.0 { "positive" } else { "negative" },
///        if pos.y >= 0.0 { "positive" } else { "negative" });
/// # }
/// ```
#[macro_export]
macro_rules! trace {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::TRACE, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        // When invoking this macro with `log`-style syntax (no fields), we
        // drop the event immediately — the `log` crate's macros don't
        // expand to an item, and if this did, it would break drop-in
        // compatibility with `log`'s macros. Since it defines no fields,
        // the handle won't be used later to add values to them.
        drop(event!(target: $target, $crate::Level::TRACE, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::TRACE, { $($k = $val),* }, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::TRACE, { $($k = $val),* }, $($arg)+)
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(target: module_path!(), $crate::Level::TRACE, { $($k = $val),* })
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(target: module_path!(), $crate::Level::TRACE, { $($k = $val),* })
    );
    ($($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::TRACE, {}, $($arg)+));
    );
}

/// Constructs an event at the debug level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// # #[derive(Debug)] struct Position { x: f32, y: f32 }
/// use tokio_trace::field;
///
/// let pos = Position { x: 3.234, y: -1.223 };
///
/// debug!(x = field::debug(pos.x), y = field::debug(pos.y));
/// debug!(target: "app_events", { position = field::debug(pos) }, "New position");
/// # }
/// ```
#[macro_export]
macro_rules! debug {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::DEBUG, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        drop(event!(target: $target, $crate::Level::DEBUG, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::DEBUG, { $($k = $val),* }, $($arg)+)
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(target: module_path!(), $crate::Level::DEBUG, { $($k = $val),* })
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(target: module_path!(), $crate::Level::DEBUG, { $($k = $val),* })
    );
    ($($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::DEBUG, {}, $($arg)+));
    );
}

/// Constructs an event at the info level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # use std::net::Ipv4Addr;
/// # fn main() {
/// # struct Connection { port: u32,  speed: f32 }
/// use tokio_trace::field;
///
/// let addr = Ipv4Addr::new(127, 0, 0, 1);
/// let conn_info = Connection { port: 40, speed: 3.20 };
///
/// info!({ port = conn_info.port }, "connected to {}", addr);
/// info!(
///     target: "connection_events",
///     ip = field::display(addr),
///     port = conn_info.port,
///     speed = field::debug(conn_info.speed)
/// );
/// # }
/// ```
#[macro_export]
macro_rules! info {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::INFO, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        drop(event!(target: $target, $crate::Level::INFO, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::INFO, { $($k = $val),* }, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::INFO, { $($k = $val),* }, $($arg)+)
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(target: module_path!(), $crate::Level::INFO, { $($k = $val),* })
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(target: module_path!(), $crate::Level::INFO, { $($k = $val),* })
    );
    ($($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::INFO, {}, $($arg)+));
    );
}

/// Constructs an event at the warn level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// use tokio_trace::field;
///
/// let warn_description = "Invalid Input";
/// let input = &[0x27, 0x45];
///
/// warn!(input = field::debug(input), warning = warn_description);
/// warn!(
///     target: "input_events",
///     { warning = warn_description },
///     "Received warning for input: {:?}", input,
/// );
/// # }
/// ```
#[macro_export]
macro_rules! warn {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::WARN, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        drop(event!(target: $target, $crate::Level::WARN, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: module_path!(),  $crate::Level::WARN, { $($k = $val),* }, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(),  $crate::Level::WARN, { $($k = $val),* }, $($arg)+)
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(target: module_path!(),  $crate::Level::WARN,{ $($k = $val),* })
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(target: module_path!(),  $crate::Level::WARN,{ $($k = $val),* })
    );
    ($($arg:tt)+ ) => (
        drop(event!(target: module_path!(),  $crate::Level::WARN, {}, $($arg)+));
    );
}

/// Constructs an event at the error level.
///
/// When both a message and fields are included, curly braces (`{` and `}`) are
/// used to delimit the list of fields from the format string for the message.
/// A trailing comma on the final field is valid.
///
/// # Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace;
/// # fn main() {
/// use tokio_trace::field;
/// let (err_info, port) = ("No connection", 22);
///
/// error!(port = port, error = field::display(err_info));
/// error!(target: "app_events", "App Error: {}", err_info);
/// error!({ info = err_info }, "error on port: {}", port);
/// # }
/// ```
#[macro_export]
macro_rules! error {
    (target: $target:expr, { $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, { $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* }, $($arg)+)
    );
    (target: $target:expr, $( $k:ident = $val:expr ),*, ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* })
    );
    (target: $target:expr, $( $k:ident = $val:expr ),* ) => (
        event!(target: $target, $crate::Level::ERROR, { $($k = $val),* })
    );
    (target: $target:expr, $($arg:tt)+ ) => (
        drop(event!(target: $target, $crate::Level::ERROR, {}, $($arg)+));
    );
    ({ $( $k:ident = $val:expr ),*, }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::ERROR, { $($k = $val),* }, $($arg)+)
    );
    ({ $( $k:ident = $val:expr ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::ERROR, { $($k = $val),* }, $($arg)+)
    );
    ($( $k:ident = $val:expr ),*, ) => (
        event!(target: module_path!(), $crate::Level::ERROR, { $($k = $val),* })
    );
    ($( $k:ident = $val:expr ),* ) => (
        event!(target: module_path!(), $crate::Level::ERROR, { $($k = $val),* })
    );
    ($($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::ERROR, {}, $($arg)+));
    );
}

#[macro_export]
// TODO: determine if this ought to be public API?
#[doc(hidden)]
macro_rules! is_enabled {
    ($callsite:expr) => {{
        let interest = $callsite.interest();
        if interest.is_never() {
            false
        } else if interest.is_always() {
            true
        } else {
            let meta = $callsite.metadata();
            $crate::dispatcher::with(|current| current.enabled(meta))
        }
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! valueset {
    ($fields:expr, $($k:ident $( = $val:expr )* ) ,*) => {
        {
            let mut iter = $fields.iter();
            $fields.value_set(&[
                $((
                    &iter.next().expect("FieldSet corrupted (this is a bug)"),
                    valueset!(@val $k $(= $val)*)
                )),*
            ])
        }
    };
    (@val $k:ident = $val:expr) => {
        Some(&$val as &$crate::field::Value)
    };
    (@val $k:ident) => { None };
}
pub mod field;
pub mod span;
pub mod subscriber;

mod sealed {
    pub trait Sealed {}
}
