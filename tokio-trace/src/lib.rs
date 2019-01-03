//! # Core Concepts
//!
//! The core of `tokio-trace`'s API is composed of `Event`s, `Span`s, and
//! `Subscriber`s. We'll cover these in turn.
//!
//! # `Span`s
//!
//! A [`Span`] represents a _period of time_ during which a program was executing
//! in some context. A thread of execution is said to _enter_ a span when it
//! begins executing in that context, and to _exit_ the span when switching to
//! another context. The span in which a thread is currently executing is
//! referred to as the _current_ span.
//!
//! Spans form a tree structure --- unless it is a root span, all spans have a
//! _parent_, and may have one or more _children_. When a new span is created,
//! the current span becomes the new span's parent. The total execution time of
//! a span consists of the time spent in that span and in the entire subtree
//! represented by its children. Thus, a parent span always lasts for at least
//! as long as the longest-executing span in its subtree.
//!
//! In addition, data may be associated with spans. A span may have _fields_ ---
//! a set of key-value pairs describing the state of the program during that
//! span; an optional name, and metadata describing the source code location
//! where the span was originally entered.
//!
//! # Events
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
//! within the context of the tree of spans that comprise a trase. Thus,
//! individual log record-like events can be pinpointed not only in time, but
//! in the logical execution flow of the system.
//!
//! Events are represented as a special case of spans --- they are created, they
//! may have fields added, and then they close immediately, without being
//! entered.
//!
//! # `Subscriber`s
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
//! [`Span`]: span/struct.Span
//! [`Event`]: struct.Event.html
//! [`Subscriber`]: subscriber/trait.Subscriber.html
//! [`observe_event`]: subscriber/trait.Subscriber.html#tymethod.observe_event
//! [`enter`]: subscriber/trait.Subscriber.html#tymethod.enter
//! [`exit`]: subscriber/trait.Subscriber.html#tymethod.exit
//! [`enabled`]: subscriber/trait.Subscriber.html#tymethod.enabled
//! [metadata]: struct.Metadata.html
extern crate tokio_trace_core;

// Somehow this `use` statement is necessary for us to re-export the `core`
// macros on Rust 1.26.0. I'm not sure how this makes it work, but it does.
#[allow(unused_imports)]
use tokio_trace_core::*;

pub use self::{
    dispatcher::Dispatch,
    field::Value,
    span::{Event, Id, Span},
    subscriber::Subscriber,
    tokio_trace_core::{
        callsite::{self, Callsite},
        metadata, Level, Metadata,
    },
};

/// Constructs a new static callsite for a span or event.
#[macro_export]
macro_rules! callsite {
    (span: $name:expr, $( $field_name:ident ),*) => ({
        callsite!(@
            name: $name,
            target: module_path!(),
            level: $crate::Level::TRACE,
            fields: &[ $(stringify!($field_name)),* ]
        )
    });
    (event: $lvl:expr, $( $field_name:ident ),*) =>
        (callsite!(event: $lvl, target: module_path!(), $( $field_name ),* ));
    (event: $lvl:expr, target: $target:expr, $( $field_name:ident ),*) => ({
        callsite!(@
            name: concat!("event at ", file!(), ":", line!()),
            target: $target,
            level: $lvl,
            fields: &[ "message", $(stringify!($field_name)),* ]
        )
    });
    (@
        name: $name:expr,
        target: $target:expr,
        level: $lvl:expr,
        fields: $field_names:expr
    ) => ({
        use std::sync::{Once, atomic::{ATOMIC_USIZE_INIT, AtomicUsize, Ordering}};
        use $crate::{callsite, Metadata, subscriber::Interest};
        struct MyCallsite;
        static META: Metadata<'static> = {
            use $crate::*;
            metadata! {
                name: $name,
                target: $target,
                level: $lvl,
                fields: $field_names,
                callsite: &MyCallsite,
            }
        };
        static INTEREST: AtomicUsize = ATOMIC_USIZE_INIT;
        static REGISTRATION: Once = Once::new();
        impl MyCallsite {
            #[inline]
            fn interest(&self) -> Interest {
                match INTEREST.load(Ordering::Relaxed) {
                    0 => Interest::NEVER,
                    2 => Interest::ALWAYS,
                    _ => Interest::SOMETIMES,
                }
            }
        }
        impl callsite::Callsite for MyCallsite {
            fn add_interest(&self, interest: Interest) {
                let current_interest = self.interest();
                let interest = match () {
                    // If the added interest is `NEVER`, don't change anything
                    // --- either a different subscriber added a higher
                    // interest, which we want to preserve, or the interest is 0
                    // anyway (as it's initialized to 0).
                    _ if interest.is_never() => return,
                    // If the interest is `SOMETIMES`, that overwrites a `NEVER`
                    // interest, but doesn't downgrade an `ALWAYS` interest.
                    _ if interest.is_sometimes() && current_interest.is_never() => 1,
                    // If the interest is `ALWAYS`, we overwrite the current
                    // interest, as ALWAYS is the highest interest level and
                    // should take precedent.
                    _ if interest.is_always() => 2,
                    _ => return,
                };
                INTEREST.store(interest, Ordering::Relaxed);
            }
            fn remove_interest(&self) {
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
/// span!("my span", foo = 2u64, bar = "a string").enter(|| {
///     // do work inside the span...
/// });
/// # }
/// ```
#[macro_export]
macro_rules! span {
    ($name:expr) => { span!($name,) };
    ($name:expr, $($k:ident $( = $val:expr )* ) ,*) => {
        {
            #[allow(unused_imports)]
            use $crate::{callsite, field::{Value, AsField}, Span};
            use $crate::callsite::Callsite;
            let callsite = callsite! { span: $name, $( $k ),* };
            let mut span = Span::new(callsite.interest(), callsite.metadata());
            // Depending on how many fields are generated, this may or may
            // not actually be used, but it doesn't make sense to repeat it.
            #[allow(unused_variables, unused_mut)] {
                if !span.is_disabled() {
                    let mut keys = callsite.metadata().fields().into_iter();
                    $(
                        let key = keys.next()
                            .expect(concat!("metadata should define a key for '", stringify!($k), "'"));
                        span!(@ record: span, $k, &key, $($val)*);
                    )*
                };
            }
            span
        }
    };
    (@ record: $span:expr, $k:expr, $i:expr, $val:expr) => (
        $span.record($i, &$val)
    );
    (@ record: $span:expr, $k:expr, $i:expr,) => (
        // skip
    );
}

#[macro_export]
macro_rules! event {
    // (target: $target:expr, $lvl:expr, { $( $k:ident $( = $val:expr )* ),* }, $fmt:expr ) => (
    //     event!(target: $target, $lvl, { $($k $( = $val)* ),* }, $fmt, )
    // );
    (target: $target:expr, $lvl:expr, { $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => ({
        {
            #[allow(unused_imports)]
            use $crate::{callsite, Id, Subscriber, Event, field::{Value, AsField}};
            use $crate::callsite::Callsite;
            let callsite = callsite! { event:
                $lvl,
                target:
                $target, $( $k ),*
            };
            let mut event = Event::new(callsite.interest(), callsite.metadata());
            // Depending on how many fields are generated, this may or may
            // not actually be used, but it doesn't make sense to repeat it.
            #[allow(unused_variables, unused_mut)] {
                if !event.is_disabled() {
                    let mut keys = callsite.metadata().fields().into_iter();
                    let msg_key = keys.next()
                        .expect("event metadata should define a key for the message");
                    event.message(&msg_key, format_args!( $($arg)+ ));
                    $(
                        let key = keys.next()
                            .expect(concat!("metadata should define a key for '", stringify!($k), "'"));
                        event!(@ record: event, $k, &key, $($val)*);
                    )*
                }
            }
            event
        }
    });
    ( $lvl:expr, { $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $lvl, { $($k $( = $val)* ),* }, $($arg)+)
    );
    ( $lvl:expr, $($arg:tt)+ ) => (
        event!(target: module_path!(), $lvl, { }, $($arg)+)
    );
    (@ record: $ev:expr, $k:expr, $i:expr, $val:expr) => (
        $ev.record($i, &$val);
    );
    (@ record: $ev:expr, $k:expr, $i:expr,) => (
        // skip
    );
}

#[macro_export]
macro_rules! trace {
    ({ $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::TRACE, { $($k $( = $val)* ),* }, $($arg)+)
    );
    ( $($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::TRACE, {}, $($arg)+));
    );
}

#[macro_export]
macro_rules! debug {
    ({ $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::DEBUG, { $($k $( = $val)* ),* }, $($arg)+)
    );
    ( $($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::DEBUG, {}, $($arg)+));
    );
}

#[macro_export]
macro_rules! info {
    ({ $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::INFO, { $($k $( = $val)* ),* }, $($arg)+)
    );
    ( $($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::INFO, {}, $($arg)+));
    );
}

#[macro_export]
macro_rules! warn {
    ({ $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::WARN, { $($k $( = $val)* ),* }, $($arg)+)
    );
    ( $($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::WARN, {}, $($arg)+));
    );
}

#[macro_export]
macro_rules! error {
    ({ $( $k:ident $( = $val:expr )* ),* }, $($arg:tt)+ ) => (
        event!(target: module_path!(), $crate::Level::ERROR, { $($k $( = $val)* ),* }, $($arg)+)
    );
    ( $($arg:tt)+ ) => (
        drop(event!(target: module_path!(), $crate::Level::ERROR, {}, $($arg)+));
    );
}

pub mod dispatcher;
pub mod field;
pub mod span;
pub mod subscriber;

mod sealed {
    pub trait Sealed {}
}
