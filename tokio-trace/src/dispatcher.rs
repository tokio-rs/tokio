//! Dispatches trace events to `Subscriber`s.
use tokio_trace_core;
pub use tokio_trace_core::dispatcher::{get_default, Dispatch};

/// Sets this dispatch as the default for the duration of a closure.
///
/// The default dispatcher is used when creating a new [`Span`] or
/// [`Event`], _if no span is currently executing_. If a span is currently
/// executing, new spans or events are dispatched to the subscriber that
/// tagged that span, instead.
///
/// [span]: ../span/struct.Span.html
/// [`Subscriber`]: ../subscriber/trait.Subscriber.html
/// [`Event`]: ../event/struct.Event.html
pub fn with_default<T>(dispatcher: impl Into<Dispatch>, f: impl FnOnce() -> T) -> T {
    tokio_trace_core::dispatcher::with_default(&dispatcher.into(), f)
}
