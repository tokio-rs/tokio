//! Collects and records trace data.
pub use tokio_trace_core::subscriber::*;

/// Sets this dispatch as the default for the duration of a closure.
///
/// The default dispatcher is used when creating a new [`Span`] or
/// [`Event`], _if no span is currently executing_. If a span is currently
/// executing, new spans or events are dispatched to the subscriber that
/// tagged that span, instead.
///
/// [`Span`]: ../span/struct.Span.html
/// [`Subscriber`]: ../subscriber/trait.Subscriber.html
/// [`Event`]: :../event/struct.Event.html
pub fn with_default<T, S>(subscriber: S, f: impl FnOnce() -> T) -> T
where
    S: Subscriber + Send + Sync + 'static,
{
    ::dispatcher::with_default(&::Dispatch::new(subscriber), f)
}
