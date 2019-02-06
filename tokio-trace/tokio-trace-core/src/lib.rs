#![deny(missing_debug_implementations, missing_docs, unreachable_pub)]
#![cfg_attr(test, deny(warnings))]

//! Core primitives for `tokio-trace`.
//!
//! `tokio-trace` is a framework for instrumenting Rust programs to collect
//! structured, event-based diagnostic information. This crate defines the core
//! primitives of `tokio-trace`.
//!
//! The crate provides:
//!
//! * [`Span`] identifies a span within the execution of a program.
//!
//! * [`Subscriber`], the trait implemented to collect trace data.
//!
//! * [`Metadata`] and [`Callsite`] provide information describing `Span`s.
//!
//! * [`Field`] and [`FieldSet`] describe and access the structured data attached to
//!   a `Span`.
//!
//! * [`Dispatch`] allows span events to be dispatched to `Subscriber`s.
//!
//! In addition, it defines the global callsite registry and per-thread current
//! dispatcher which other components of the tracing system rely on.
//!
//! Application authors will typically not use this crate directly. Instead,
//! they will use the `tokio-trace` crate, which provides a much more
//! fully-featured API. However, this crate's API will change very infrequently,
//! so it may be used when dependencies must be very stable.
//!
//! [`Span`]: ::span::Span
//! [`Subscriber`]: ::subscriber::Subscriber
//! [`Metadata`]: ::metadata::Metadata
//! [`Callsite`]: ::callsite::Callsite
//! [`Field`]: ::field::Field
//! [`FieldSet`]: ::field::FieldSet
//! [`Dispatch`]: ::dispatcher::Dispatch
//!

#[macro_use]
extern crate lazy_static;

/// Statically constructs an [`Identifier`] for the provided [`Callsite`].
///
/// This may be used in contexts, such as static initializers, where the
/// [`Callsite::id`] function is not currently usable.
///
/// For example:
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace_core;
/// use tokio_trace_core::callsite;
/// # use tokio_trace_core::{Metadata, subscriber::Interest};
/// # fn main() {
/// pub struct MyCallsite {
///    // ...
/// }
/// impl callsite::Callsite for MyCallsite {
/// # fn add_interest(&self, _: Interest) { unimplemented!() }
/// # fn clear_interest(&self) {}
/// # fn metadata(&self) -> &Metadata { unimplemented!() }
///     // ...
/// }
///
/// static CALLSITE: MyCallsite = MyCallsite {
///     // ...
/// };
///
/// static CALLSITE_ID: callsite::Identifier = identify_callsite!(&CALLSITE);
/// # }
/// ```
///
/// [`Identifier`]: ::callsite::Identifier
/// [`Callsite`]: ::callsite::Callsite
/// [`Callsite::id`]: ::callsite::Callsite::id
#[macro_export]
macro_rules! identify_callsite {
    ($callsite:expr) => {
        $crate::callsite::Identifier($callsite)
    };
}

/// Statically constructs new span [metadata].
///
/// This may be used in contexts, such as static initializers, where the
/// [`Metadata::new`] function is not currently usable.
///
/// /// For example:
/// ```rust
/// # #[macro_use]
/// # extern crate tokio_trace_core;
/// # use tokio_trace_core::{callsite::Callsite, subscriber::Interest};
/// use tokio_trace_core::{Metadata, Level};
/// # fn main() {
/// # pub struct MyCallsite { }
/// # impl Callsite for MyCallsite {
/// # fn add_interest(&self, _: Interest) { unimplemented!() }
/// # fn clear_interest(&self) {}
/// # fn metadata(&self) -> &Metadata { unimplemented!() }
/// # }
/// #
/// static FOO_CALLSITE: MyCallsite = MyCallsite {
///     // ...
/// };
///
/// static FOO_METADATA: Metadata = metadata!{
///     name: "foo",
///     target: module_path!(),
///     level: Level::DEBUG,
///     fields: &["bar", "baz"],
///     callsite: &FOO_CALLSITE,
/// };
/// # }
/// ```
///
/// [metadata]: ::metadata::Metadata
/// [`Metadata::new`]: ::metadata::Metadata::new
#[macro_export]
macro_rules! metadata {
    (
        name: $name:expr,
        target: $target:expr,
        level: $level:expr,
        fields: $fields:expr,
        callsite: $callsite:expr
    ) => {
        metadata! {
            name: $name,
            target: $target,
            level: $level,
            fields: $fields,
            callsite: $callsite,
        }
    };
    (
        name: $name:expr,
        target: $target:expr,
        level: $level:expr,
        fields: $fields:expr,
        callsite: $callsite:expr,
    ) => {
        $crate::metadata::Metadata {
            name: $name,
            target: $target,
            level: $level,
            file: Some(file!()),
            line: Some(line!()),
            module_path: Some(module_path!()),
            fields: $crate::field::FieldSet {
                names: $fields,
                callsite: identify_callsite!($callsite),
            },
        }
    };
}

pub mod callsite;
pub mod dispatcher;
pub mod event;
pub mod field;
pub mod metadata;
pub mod span;
pub mod subscriber;

pub use self::{
    callsite::Callsite,
    dispatcher::Dispatch,
    event::Event,
    field::Field,
    metadata::{Level, Metadata},
    span::Span,
    subscriber::{Interest, Subscriber},
};

mod sealed {
    pub trait Sealed {}
}
