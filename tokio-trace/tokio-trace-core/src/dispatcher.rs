//! Dispatches trace events to `Subscriber`s.
use {
    callsite, field,
    subscriber::{self, Subscriber},
    Metadata, Span,
};

use std::{
    cell::RefCell,
    fmt,
    sync::{Arc, Weak},
};

/// `Dispatch` trace data to a [`Subscriber`](::Subscriber).
#[derive(Clone)]
pub struct Dispatch {
    subscriber: Arc<Subscriber + Send + Sync>,
}

thread_local! {
    static CURRENT_DISPATCH: RefCell<Dispatch> = RefCell::new(Dispatch::none());
}

/// Sets this dispatch as the default for the duration of a closure.
///
/// The default dispatcher is used when creating a new [`Span`] or
/// [`Event`], _if no span is currently executing_. If a span is currently
/// executing, new spans or events are dispatched to the subscriber that
/// tagged that span, instead.
///
/// [`Span`]: ::span::Span
/// [`Subscriber`]: ::Subscriber
/// [`Event`]: ::Event
pub fn with_default<T>(dispatcher: Dispatch, f: impl FnOnce() -> T) -> T {
    // A drop guard that resets CURRENT_DISPATCH to the prior dispatcher.
    // Using this (rather than simply resetting after calling `f`) ensures
    // that we always reset to the prior dispatcher even if `f` panics.
    struct ResetGuard(Option<Dispatch>);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            if let Some(dispatch) = self.0.take() {
                let _ = CURRENT_DISPATCH.try_with(|current| {
                    *current.borrow_mut() = dispatch;
                });
            }
        }
    }

    let prior = CURRENT_DISPATCH.try_with(|current| current.replace(dispatcher));
    let _guard = ResetGuard(prior.ok());
    f()
}

/// Executes a closure with a reference to this thread's current dispatcher.
pub fn with<T, F>(mut f: F) -> T
where
    F: FnMut(&Dispatch) -> T,
{
    CURRENT_DISPATCH
        .try_with(|current| f(&*current.borrow()))
        .unwrap_or_else(|_| f(&Dispatch::none()))
}

pub(crate) struct Registrar(Weak<Subscriber + Send + Sync>);

impl Dispatch {
    /// Returns a new `Dispatch` that discards events and spans.
    pub fn none() -> Self {
        Dispatch {
            subscriber: Arc::new(NoSubscriber),
        }
    }

    /// Returns a `Dispatch` to the given [`Subscriber`](::Subscriber).
    pub fn new<S>(subscriber: S) -> Self
    // TODO: Add some kind of `UnsyncDispatch`?
    where
        S: Subscriber + Send + Sync + 'static,
    {
        let me = Dispatch {
            subscriber: Arc::new(subscriber),
        };
        callsite::register_dispatch(&me);
        me
    }

    pub(crate) fn registrar(&self) -> Registrar {
        Registrar(Arc::downgrade(&self.subscriber))
    }
}

impl fmt::Debug for Dispatch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("Dispatch(...)")
    }
}

impl Subscriber for Dispatch {
    #[inline]
    fn register_callsite(&self, metadata: &Metadata) -> subscriber::Interest {
        self.subscriber.register_callsite(metadata)
    }

    #[inline]
    fn new_span(&self, metadata: &Metadata) -> Span {
        self.subscriber.new_span(metadata)
    }

    #[inline]
    fn record_i64(&self, span: &Span, field: &field::Field, value: i64) {
        self.subscriber.record_i64(span, field, value)
    }

    #[inline]
    fn record_u64(&self, span: &Span, field: &field::Field, value: u64) {
        self.subscriber.record_u64(span, field, value)
    }

    #[inline]
    fn record_bool(&self, span: &Span, field: &field::Field, value: bool) {
        self.subscriber.record_bool(span, field, value)
    }

    #[inline]
    fn record_str(&self, span: &Span, field: &field::Field, value: &str) {
        self.subscriber.record_str(span, field, value)
    }

    #[inline]
    fn record_debug(&self, span: &Span, field: &field::Field, value: &fmt::Debug) {
        self.subscriber.record_debug(span, field, value)
    }

    #[inline]
    fn add_follows_from(&self, span: &Span, follows: Span) {
        self.subscriber.add_follows_from(span, follows)
    }

    #[inline]
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.subscriber.enabled(metadata)
    }

    #[inline]
    fn enter(&self, span: &Span) {
        self.subscriber.enter(span)
    }

    #[inline]
    fn exit(&self, span: &Span) {
        self.subscriber.exit(span)
    }

    #[inline]
    fn clone_span(&self, id: &Span) -> Span {
        self.subscriber.clone_span(&id)
    }

    #[inline]
    fn drop_span(&self, id: Span) {
        self.subscriber.drop_span(id)
    }
}

struct NoSubscriber;
impl Subscriber for NoSubscriber {
    #[inline]
    fn register_callsite(&self, _: &Metadata) -> subscriber::Interest {
        subscriber::Interest::never()
    }

    fn new_span(&self, _meta: &Metadata) -> Span {
        Span::from_u64(0)
    }

    fn record_debug(&self, _span: &Span, _field: &field::Field, _value: &fmt::Debug) {}

    fn add_follows_from(&self, _span: &Span, _follows: Span) {}

    #[inline]
    fn enabled(&self, _metadata: &Metadata) -> bool {
        false
    }

    fn enter(&self, _span: &Span) {}
    fn exit(&self, _span: &Span) {}
}

impl Registrar {
    pub(crate) fn try_register(&self, metadata: &Metadata) -> Option<subscriber::Interest> {
        self.0.upgrade().map(|s| s.register_callsite(metadata))
    }
}
