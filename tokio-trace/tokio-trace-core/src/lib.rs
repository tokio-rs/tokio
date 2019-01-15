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
/// [metadata]: ::metadata::Metadata
/// [`Metadata::new`]: ::metadata::Metadata::new
#[macro_export]
macro_rules! metadata {
    (
        name: $name:expr,
        target: $target:expr,
        level: $level:expr,
        fields: $field:expr,
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
pub mod field;
pub mod metadata;
pub mod span;
pub mod subscriber;

pub use self::{
    callsite::Callsite,
    dispatcher::Dispatch,
    field::Field,
    metadata::{Level, Metadata},
    span::Span,
    subscriber::{Interest, Subscriber},
};
