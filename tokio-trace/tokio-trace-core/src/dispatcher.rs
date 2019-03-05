//! Dispatches trace events to `Subscriber`s.
use {
    callsite, span,
    subscriber::{self, Subscriber},
    Event, Metadata,
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
/// The default dispatcher is used when creating a new [span] or
/// [`Event`], _if no span is currently executing_. If a span is currently
/// executing, new spans or events are dispatched to the subscriber that
/// tagged that span, instead.
///
/// [span]: ../span/index.html
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

    /// Registers a new callsite with this subscriber, returning whether or not
    /// the subscriber is interested in being notified about the callsite.
    ///
    /// This calls the [`register_callsite`](::Subscriber::register_callsite)
    /// function on the `Subscriber` that this `Dispatch` forwards to.
    #[inline]
    pub fn register_callsite(&self, metadata: &Metadata) -> subscriber::Interest {
        self.subscriber.register_callsite(metadata)
    }

    /// Visit the construction of a new span, returning a new [ID] for the
    /// span being constructed.
    ///
    /// This calls the [`new_span`](::Subscriber::new_span)
    /// function on the `Subscriber` that this `Dispatch` forwards to.
    ///
    /// [ID]: ../span/struct.Id.html
    #[inline]
    pub fn new_span(&self, span: &span::Attributes) -> span::Id {
        self.subscriber.new_span(span)
    }

    /// Visit a set of values on a span.
    ///
    /// This calls the [`record`](::Subscriber::record)
    /// function on the `Subscriber` that this `Dispatch` forwards to.
    #[inline]
    pub fn record(&self, span: &span::Id, values: &span::Record) {
        self.subscriber.record(span, &values)
    }

    /// Adds an indication that `span` follows from the span with the id
    /// `follows`.
    ///
    /// This calls the [`record_follows_from`](::Subscriber::record_follows_from)
    /// function on the `Subscriber` that this `Dispatch` forwards to.
    #[inline]
    pub fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
        self.subscriber.record_follows_from(span, follows)
    }

    /// Returns true if a span with the specified [metadata] would be
    /// recorded.
    ///
    /// This calls the [`enabled`](::Subscriber::enabled) function on
    /// the `Subscriber` that this `Dispatch` forwards to.
    ///
    /// [metadata]: ::Metadata
    #[inline]
    pub fn enabled(&self, metadata: &Metadata) -> bool {
        self.subscriber.enabled(metadata)
    }

    /// Visits that an [`Event`] has occurred.
    ///
    /// This calls the [`event`](::Subscriber::event) function on
    /// the `Subscriber` that this `Dispatch` forwards to.
    ///
    /// [`Event`]: ::event::Event
    #[inline]
    pub fn event(&self, event: &Event) {
        self.subscriber.event(event)
    }

    /// Visits that a span has been entered.
    ///
    /// This calls the [`enter`](::Subscriber::enter) function on the
    /// `Subscriber` that this `Dispatch` forwards to.
    #[inline]
    pub fn enter(&self, span: &span::Id) {
        self.subscriber.enter(span)
    }

    /// Visits that a span has been exited.
    ///
    /// This calls the [`exit`](::Subscriber::exit) function on the `Subscriber`
    /// that this `Dispatch` forwards to.
    #[inline]
    pub fn exit(&self, span: &span::Id) {
        self.subscriber.exit(span)
    }

    /// Notifies the subscriber that a [span ID] has been cloned.
    ///
    /// This function is guaranteed to only be called with span IDs that were
    /// returned by this `Dispatch`'s `new_span` function.
    ///
    /// This calls the [`clone_span`](::Subscriber::clone_span) function on
    /// the `Subscriber` that this `Dispatch` forwards to.
    ///
    /// [span ID]: ../span/struct.Id.html
    #[inline]
    pub fn clone_span(&self, id: &span::Id) -> span::Id {
        self.subscriber.clone_span(&id)
    }

    /// Notifies the subscriber that a [span ID] handle has been dropped.
    ///
    /// This function is guaranteed to only be called with span IDs that were
    /// returned by this `Dispatch`'s `new_span` function.
    ///
    /// This calls the [`drop_span`](::Subscriber::drop_span) function on
    /// the `Subscriber` that this `Dispatch` forwards to.
    ///
    /// [span ID]: ../span/struct.Id.html
    #[inline]
    pub fn drop_span(&self, id: span::Id) {
        self.subscriber.drop_span(id)
    }
}

impl fmt::Debug for Dispatch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("Dispatch(...)")
    }
}

impl<S> From<S> for Dispatch
where
    S: Subscriber + Send + Sync + 'static,
{
    #[inline]
    fn from(subscriber: S) -> Self {
        Dispatch::new(subscriber)
    }
}

struct NoSubscriber;
impl Subscriber for NoSubscriber {
    #[inline]
    fn register_callsite(&self, _: &Metadata) -> subscriber::Interest {
        subscriber::Interest::never()
    }

    fn new_span(&self, _: &span::Attributes) -> span::Id {
        span::Id::from_u64(0)
    }

    fn event(&self, _event: &Event) {}

    fn record(&self, _span: &span::Id, _values: &span::Record) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    #[inline]
    fn enabled(&self, _metadata: &Metadata) -> bool {
        false
    }

    fn enter(&self, _span: &span::Id) {}
    fn exit(&self, _span: &span::Id) {}
}

impl Registrar {
    pub(crate) fn try_register(&self, metadata: &Metadata) -> Option<subscriber::Interest> {
        self.0.upgrade().map(|s| s.register_callsite(metadata))
    }
}
