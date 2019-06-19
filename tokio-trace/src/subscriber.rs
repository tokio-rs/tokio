//! Collects and records trace data.
pub use tokio_trace_core::subscriber::*;

/// Sets this subscriber as the default for the duration of a closure.
///
/// The default subscriber is used when creating a new [`Span`] or
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

/// Sets this subscriber as the global default for the duration of the entire program.
/// Will be used as a fallback if no thread-local subscriber has been set in a thread (using `with_default`.)
///
/// Can only be set once; subsequent attempts to set the global default will fail.
/// Returns whether the initialization was successful.
///
/// Note: Libraries should *NOT* call `set_global_default()`! That will cause conflicts when
/// executables try to set them later.
///
/// [span]: ../span/index.html
/// [`Subscriber`]: ../subscriber/trait.Subscriber.html
/// [`Event`]: ../event/struct.Event.html
pub fn set_global_default<S>(subscriber: S) -> Result<(), SetGlobalDefaultError>
where
    S: Subscriber + Send + Sync + 'static,
{
    ::dispatcher::set_global_default(::Dispatch::new(subscriber))
}

pub use tokio_trace_core::dispatcher::SetGlobalDefaultError;
