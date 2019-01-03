//! Spans represent periods of time in the execution of a program.
//!
//! # Entering a Span
//!
//! A thread of execution is said to _enter_ a span when it begins executing,
//! and _exit_ the span when it switches to another context. Spans may be
//! entered through the [`enter`](`Span::enter`) method, which enters the target span,
//! performs a given function (either a closure or a function pointer), exits
//! the span, and then returns the result.
//!
//! Calling `enter` on a span handle enters the span that handle corresponds to,
//! if the span exists:
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # fn main() {
//! let my_var: u64 = 5;
//! let mut my_span = span!("my_span", my_var = &my_var);
//!
//! my_span.enter(|| {
//!     // perform some work in the context of `my_span`...
//! });
//!
//! // Perform some work outside of the context of `my_span`...
//!
//! my_span.enter(|| {
//!     // Perform some more work in the context of `my_span`.
//! });
//! # }
//! ```
//!
//! # The Span Lifecycle
//!
//! Execution may enter and exit a span multiple times before that
//! span is _closed_. Consider, for example, a future which has an associated
//! span and enters that span every time it is polled:
//! ```rust
//! # extern crate tokio_trace;
//! # extern crate futures;
//! # use futures::{Future, Poll, Async};
//! struct MyFuture<'a> {
//!    // data
//!    span: tokio_trace::Span<'a>,
//! }
//!
//! impl<'a> Future for MyFuture<'a> {
//!     type Item = ();
//!     type Error = ();
//!
//!     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//!         self.span.enter(|| {
//!             // Do actual future work
//! # Ok(Async::Ready(()))
//!         })
//!     }
//! }
//! ```
//!
//! If this future was spawned on an executor, it might yield one or more times
//! before `poll` returns `Ok(Async::Ready)`. If the future were to yield, then
//! the executor would move on to poll the next future, which may _also_ enter
//! an associated span or series of spans. Therefore, it is valid for a span to
//! be entered repeatedly before it completes. Only the time when that span or
//! one of its children was the current span is considered to be time spent in
//! that span. A span which is not executing and has not yet been closed is said
//! to be _idle_.
//!
//! Because spans may be entered and exited multiple times before they close,
//! [`Subscriber`]s have separate trait methods which are called to notify them
//! of span exits and when span handles are dropped. When execution exits a
//! span, [`exit`](::Subscriber::exit) will always be called with that span's ID
//! to notify the subscriber that the span has been exited. When span handles
//! are dropped, the [`drop_span`](::Subscriber::drop_span) method is called
//! with that span's ID. The subscriber may use this to determine whether or not
//! the span will be entered again.
//!
//! If there is only a single handle with the capacity to exit a span, dropping
//! that handle "close" the span, since the capacity to enter it no longer
//! exists. For example:
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # fn main() {
//! {
//!     span!("my_span").enter(|| {
//!         // perform some work in the context of `my_span`...
//!     }); // --> Subscriber::exit(my_span)
//!
//!     // The handle to `my_span` only lives inside of this block; when it is
//!     // dropped, the subscriber will be informed that `my_span` has closed.
//!
//! } // --> Subscriber::close(my_span)
//! # }
//! ```
//!
//! A span may be explicitly closed before when the span handle is dropped by
//! calling the [`Span::close`] method. Doing so will drop that handle the next
//! time it is exited. For example:
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # fn main() {
//! use tokio_trace::Span;
//!
//! let mut my_span = span!("my_span");
//! // Signal to my_span that it should close when it exits
//! my_span.close();
//! my_span.enter(|| {
//!    // ...
//! }); // --> Subscriber::exit(my_span); Subscriber::drop_span(my_span)
//!
//! // The handle to `my_span` still exists, but it now knows that the span was
//! // closed while it was executing.
//! my_span.is_closed(); // ==> true
//!
//! // Attempting to enter the span using the handle again will do nothing.
//! my_span.enter(|| {
//!     // no-op
//! });
//! # }
//! ```
//! However, if multiple handles exist, the span can still be re-entered even if
//! one or more is dropped. For determining when _all_ handles to a span have
//! been dropped, `Subscriber`s have a [`clone_span`](::Subscriber::clone_span)
//! method, which is called every time a span handle is cloned. Combined with
//! `drop_span`, this may be used to track the number of handles to a given span
//! --- if `drop_span` has been called one more time than the number of calls to
//! `clone_span` for a given ID, then no more handles to the span with that ID
//! exist. The subscriber may then treat it as closed.
//!
//! # Accessing a Span's Attributes
//!
//! The [`Attributes`] type represents a *non-entering* reference to a `Span`'s data
//! --- a set of key-value pairs (known as _fields_), a creation timestamp,
//! a reference to the span's parent in the trace tree, and metadata describing
//! the source code location where the span was created. This data is provided
//! to the [`Subscriber`] when the span is created; it may then choose to cache
//! the data for future use, record it in some manner, or discard it completely.
//!
//! [`Subscriber`]: ::Subscriber
// TODO: remove this re-export?
pub use tokio_trace_core::span::Span as Id;

use std::{
    borrow::Borrow,
    cmp, fmt,
    hash::{Hash, Hasher},
};
use {
    dispatcher::{self, Dispatch},
    field,
    subscriber::{Interest, Subscriber},
    Metadata,
};

/// A handle representing a span, with the capability to enter the span if it
/// exists.
///
/// If the span was rejected by the current `Subscriber`'s filter, entering the
/// span will silently do nothing. Thus, the handle can be used in the same
/// manner regardless of whether or not the trace is currently being collected.
#[derive(Clone, PartialEq, Hash)]
pub struct Span<'a> {
    /// A handle used to enter the span when it is not executing.
    ///
    /// If this is `None`, then the span has either closed or was never enabled.
    inner: Option<Enter<'a>>,

    /// Set to `true` when the span closes.
    ///
    /// This allows us to distinguish if `inner` is `None` because the span was
    /// never enabled (and thus the inner state was never created), or if the
    /// previously entered, but it is now closed.
    is_closed: bool,
}

/// `Event`s represent single points in time where something occurred during the
/// execution of a program.
///
/// An event can be compared to a log record in unstructured logging, but with
/// two key differences:
/// - Events exist _within the context of a [`Span`]_. Unlike log lines, they may
///   be located within the trace tree, allowing visibility into the context in
///   which the event occurred.
/// - Events have structured key-value data known as _fields_, as well as a
///   textual message. In general, a majority of the data associated with an
///   event should be in the event's fields rather than in the textual message,
///   as the fields are more structed.
///
/// [`Span`]: ::span::Span
#[derive(PartialEq, Hash)]
pub struct Event<'a> {
    /// A handle used to enter the span when it is not executing.
    ///
    /// If this is `None`, then the span has either closed or was never enabled.
    inner: Option<Enter<'a>>,
}

/// A handle representing the capacity to enter a span which is known to exist.
///
/// Unlike `Span`, this type is only constructed for spans which _have_ been
/// enabled by the current filter. This type is primarily used for implementing
/// span handles; users should typically not need to interact with it directly.
#[derive(Debug)]
pub(crate) struct Enter<'a> {
    /// The span's ID, as provided by `subscriber`.
    id: Id,

    /// The subscriber that will receive events relating to this span.
    ///
    /// This should be the same subscriber that provided this span with its
    /// `id`.
    subscriber: Dispatch,

    /// A flag indicating that the span has been instructed to close when
    /// possible.
    closed: bool,

    meta: &'a Metadata<'a>,
}

/// A guard representing a span which has been entered and is currently
/// executing.
///
/// This guard may be used to exit the span, returning an `Enter` to
/// re-enter it.
///
/// This type is primarily used for implementing span handles; users should
/// typically not need to interact with it directly.
#[derive(Debug)]
#[must_use = "once a span has been entered, it should be exited"]
struct Entered<'a> {
    inner: Enter<'a>,
}

// ===== impl Span =====

impl<'a> Span<'a> {
    /// Constructs a new `Span` originating from the given [`Callsite`].
    ///
    /// The new span will be constructed by the currently-active [`Subscriber`],
    /// with the [current span] as its parent (if one exists).
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [`Callsite`]: ::callsite::Callsite
    /// [`Subscriber`]: ::subscriber::Subscriber
    /// [current span]: ::span::Span::current
    /// [field values]: ::span::Span::record
    /// [`follows_from`]: ::span::Span::follows_from
    #[inline]
    pub fn new(interest: Interest, meta: &'a Metadata<'a>) -> Span<'a> {
        if interest.is_never() {
            return Span::new_disabled();
        }
        dispatcher::with_current(|dispatch| {
            if interest.is_sometimes() && !dispatch.enabled(meta) {
                return Span {
                    inner: None,
                    is_closed: false,
                };
            }
            let id = dispatch.new_span(meta);
            let inner = Some(Enter::new(id, dispatch, meta));
            Self {
                inner,
                is_closed: false,
            }
        })
    }

    /// Constructs a new disabled span.
    pub fn new_disabled() -> Span<'a> {
        Span {
            inner: None,
            is_closed: false,
        }
    }

    /// Executes the given function in the context of this span.
    ///
    /// If this span is enabled, then this function enters the span, invokes
    /// and then exits the span. If the span is disabled, `f` will still be
    /// invoked, but in the context of the currently-executing span (if there is
    /// one).
    ///
    /// Returns the result of evaluating `f`.
    pub fn enter<F: FnOnce() -> T, T>(&mut self, f: F) -> T {
        match self.inner.take() {
            Some(inner) => dispatcher::with_default(inner.subscriber.clone(), || {
                let guard = inner.enter();
                let result = f();
                self.inner = guard.exit();
                result
            }),
            None => f(),
        }
    }

    /// Returns a [`Field`](::field::Field) for the field with the given `name`, if
    /// one exists,
    pub fn field_named<Q>(&self, name: &Q) -> Option<field::Field>
    where
        Q: Borrow<str>,
    {
        self.inner
            .as_ref()
            .and_then(|inner| inner.meta.fields().field_named(name))
    }

    /// Returns true if this `Span` has a field for the given
    /// [`Field`](::field::Field) or field name.
    pub fn has_field<Q: ?Sized>(&self, field: &Q) -> bool
    where
        Q: field::AsField,
    {
        self.metadata()
            .and_then(|meta| field.as_field(meta))
            .is_some()
    }

    /// Records that the field described by `field` has the value `value`.
    pub fn record<Q: ?Sized, V: ?Sized>(&mut self, field: &Q, value: &V) -> &mut Self
    where
        Q: field::AsField,
        V: field::Value,
    {
        if let Some(ref mut inner) = self.inner {
            value.record(field, inner);
        }
        self
    }

    /// Signals that this span should close the next time it is exited, or when
    /// it is dropped.
    pub fn close(&mut self) {
        if let Some(ref mut inner) = self.inner {
            inner.close();
        }
        self.is_closed = true;
    }

    /// Returns `true` if this span is closed.
    pub fn is_closed(&self) -> bool {
        self.is_closed
    }

    /// Returns `true` if this span was disabled by the subscriber and does not
    /// exist.
    pub fn is_disabled(&self) -> bool {
        self.inner.is_none() && !self.is_closed
    }

    /// Indicates that the span with the given ID has an indirect causal
    /// relationship with this span.
    ///
    /// This relationship differs somewhat from the parent-child relationship: a
    /// span may have any number of prior spans, rather than a single one; and
    /// spans are not considered to be executing _inside_ of the spans they
    /// follow from. This means that a span may close even if subsequent spans
    /// that follow from it are still open, and time spent inside of a
    /// subsequent span should not be included in the time its precedents were
    /// executing. This is used to model causal relationships such as when a
    /// single future spawns several related background tasks, et cetera.
    ///
    /// If this span is disabled, or the resulting follows-from relationship
    /// would be invalid, this function will do nothing.
    pub fn follows_from(&self, from: Id) -> &Self {
        if let Some(ref inner) = self.inner {
            inner.follows_from(from);
        }
        self
    }

    /// Returns this span's `Id`, if it is enabled.
    pub fn id(&self) -> Option<Id> {
        self.inner.as_ref().map(Enter::id)
    }

    /// Returns this span's `Metadata`, if it is enabled.
    pub fn metadata(&self) -> Option<&'a Metadata<'a>> {
        self.inner.as_ref().map(|inner| inner.metadata())
    }
}

impl<'a> fmt::Debug for Span<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut span = f.debug_struct("Span");
        if let Some(ref inner) = self.inner {
            span.field("id", &inner.id())
        } else {
            span.field("disabled", &true)
        }
        .finish()
    }
}

// ===== impl Event =====

impl<'a> Event<'a> {
    /// Constructs a new `Event` originating from the given [`Callsite`].
    ///
    /// The new span will be constructed by the currently-active [`Subscriber`],
    /// with the [current span] as its parent (if one exists).
    ///
    /// After the event is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [`Callsite`]: ::callsite::Callsite
    /// [`Subscriber`]: ::subscriber::Subscriber
    /// [current span]: ::span::Span::current
    /// [field values]: ::span::Evemt::record
    /// [`follows_from`]: ::span::Event::follows_from
    #[inline]
    pub fn new(interest: Interest, meta: &'a Metadata<'a>) -> Self {
        if interest.is_never() {
            return Self { inner: None };
        }
        dispatcher::with_current(|dispatch| {
            if interest.is_sometimes() && !dispatch.enabled(meta) {
                return Self { inner: None };
            }
            let id = dispatch.new_span(meta);
            let inner = Enter::new(id, dispatch, meta);
            Self { inner: Some(inner) }
        })
    }

    /// Adds a formattable message describing the event that occurred.
    pub fn message(&mut self, key: &field::Field, message: fmt::Arguments) -> &mut Self {
        if let Some(ref mut inner) = self.inner {
            inner.subscriber.record_debug(&inner.id, key, &message);
        }
        self
    }

    /// Returns a [`Field`](::field::Field) for the field with the given `name`, if
    /// one exists,
    pub fn field_named<Q>(&self, name: &Q) -> Option<field::Field>
    where
        Q: Borrow<str>,
    {
        self.inner
            .as_ref()
            .and_then(|inner| inner.meta.fields().field_named(name))
    }

    /// Returns true if this `Event` has a field for the given
    /// [`Field`](::field::Field) or field name.
    pub fn has_field<Q: ?Sized>(&self, field: &Q) -> bool
    where
        Q: field::AsField,
    {
        self.metadata()
            .and_then(|meta| field.as_field(meta))
            .is_some()
    }

    /// Records that the field described by `field` has the value `value`.
    pub fn record<Q: ?Sized, V: ?Sized>(&mut self, field: &Q, value: &V) -> &mut Self
    where
        Q: field::AsField,
        V: field::Value,
    {
        if let Some(ref mut inner) = self.inner {
            value.record(field, inner);
        }
        self
    }

    /// Returns `true` if this span was disabled by the subscriber and does not
    /// exist.
    pub fn is_disabled(&self) -> bool {
        self.inner.is_none()
    }

    /// Indicates that the span with the given ID has an indirect causal
    /// relationship with this event.
    ///
    /// This relationship differs somewhat from the parent-child relationship: a
    /// span may have any number of prior spans, rather than a single one; and
    /// spans are not considered to be executing _inside_ of the spans they
    /// follow from. This means that a span may close even if subsequent spans
    /// that follow from it are still open, and time spent inside of a
    /// subsequent span should not be included in the time its precedents were
    /// executing. This is used to model causal relationships such as when a
    /// single future spawns several related background tasks, et cetera.
    ///
    /// If this event is disabled, or the resulting follows-from relationship
    /// would be invalid, this function will do nothing.
    pub fn follows_from(&self, from: Id) -> &Self {
        if let Some(ref inner) = self.inner {
            inner.follows_from(from);
        }
        self
    }

    /// Returns this span's `Id`, if it is enabled.
    pub fn id(&self) -> Option<Id> {
        self.inner.as_ref().map(Enter::id)
    }

    /// Returns this span's `Metadata`, if it is enabled.
    pub fn metadata(&self) -> Option<&'a Metadata<'a>> {
        self.inner.as_ref().map(|inner| inner.metadata())
    }
}

// ===== impl Enter =====

impl<'a> Enter<'a> {
    /// Indicates that this handle will not be reused to enter the span again.
    ///
    /// After calling `close`, the `Entered` guard returned by `self.enter()`
    /// will _drop_ this handle when it is exited.
    fn close(&mut self) {
        self.closed = true;
    }

    /// Enters the span, returning a guard that may be used to exit the span and
    /// re-enter the prior span.
    ///
    /// This is used internally to implement `Span::enter`. It may be used for
    /// writing custom span handles, but should generally not be called directly
    /// when entering a span.
    fn enter(self) -> Entered<'a> {
        self.subscriber.enter(&self.id);
        Entered { inner: self }
    }

    /// Indicates that the span with the given ID has an indirect causal
    /// relationship with this span.
    ///
    /// This relationship differs somewhat from the parent-child relationship: a
    /// span may have any number of prior spans, rather than a single one; and
    /// spans are not considered to be executing _inside_ of the spans they
    /// follow from. This means that a span may close even if subsequent spans
    /// that follow from it are still open, and time spent inside of a
    /// subsequent span should not be included in the time its precedents were
    /// executing. This is used to model causal relationships such as when a
    /// single future spawns several related background tasks, et cetera.
    ///
    /// If this span is disabled, this function will do nothing. Otherwise, it
    /// returns `Ok(())` if the other span was added as a precedent of this
    /// span, or an error if this was not possible.
    fn follows_from(&self, from: Id) {
        self.subscriber.add_follows_from(&self.id, from)
    }

    /// Returns the span's ID.
    fn id(&self) -> Id {
        self.id.clone()
    }

    /// Returns the span's metadata.
    fn metadata(&self) -> &'a Metadata<'a> {
        self.meta
    }

    /// Record a signed 64-bit integer value.
    fn record_value_i64(&self, field: &field::Field, value: i64) {
        self.subscriber.record_i64(&self.id, field, value)
    }

    /// Record an unsigned 64-bit integer value.
    fn record_value_u64(&self, field: &field::Field, value: u64) {
        self.subscriber.record_u64(&self.id, field, value)
    }

    /// Record a boolean value.
    fn record_value_bool(&self, field: &field::Field, value: bool) {
        self.subscriber.record_bool(&self.id, field, value)
    }

    /// Record a string value.
    fn record_value_str(&self, field: &field::Field, value: &str) {
        self.subscriber.record_str(&self.id, field, value)
    }

    /// Record a value implementing `fmt::Debug`.
    fn record_value_debug(&self, field: &field::Field, value: &fmt::Debug) {
        self.subscriber.record_debug(&self.id, field, value)
    }

    fn new(id: Id, subscriber: &Dispatch, meta: &'a Metadata<'a>) -> Self {
        Self {
            id,
            subscriber: subscriber.clone(),
            closed: false,
            meta,
        }
    }
}

impl<'a> cmp::PartialEq for Enter<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> Hash for Enter<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<'a> Drop for Enter<'a> {
    fn drop(&mut self) {
        self.subscriber.drop_span(self.id.clone());
    }
}

impl<'a> Clone for Enter<'a> {
    fn clone(&self) -> Self {
        Self {
            id: self.subscriber.clone_span(&self.id),
            subscriber: self.subscriber.clone(),
            closed: self.closed,
            meta: self.meta,
        }
    }
}

impl<'a> field::Record for Enter<'a> {
    #[inline]
    fn record_i64<Q: ?Sized>(&mut self, field: &Q, value: i64)
    where
        Q: field::AsField,
    {
        if let Some(key) = field.as_field(self.metadata()) {
            self.record_value_i64(&key, value);
        }
    }

    #[inline]
    fn record_u64<Q: ?Sized>(&mut self, field: &Q, value: u64)
    where
        Q: field::AsField,
    {
        if let Some(key) = field.as_field(self.metadata()) {
            self.record_value_u64(&key, value);
        }
    }

    #[inline]
    fn record_bool<Q: ?Sized>(&mut self, field: &Q, value: bool)
    where
        Q: field::AsField,
    {
        if let Some(key) = field.as_field(self.metadata()) {
            self.record_value_bool(&key, value);
        }
    }

    #[inline]
    fn record_str<Q: ?Sized>(&mut self, field: &Q, value: &str)
    where
        Q: field::AsField,
    {
        if let Some(key) = field.as_field(self.metadata()) {
            self.record_value_str(&key, value);
        }
    }

    #[inline]
    fn record_debug<Q: ?Sized>(&mut self, field: &Q, value: &fmt::Debug)
    where
        Q: field::AsField,
    {
        if let Some(key) = field.as_field(self.metadata()) {
            self.record_value_debug(&key, value);
        }
    }
}

// ===== impl Entered =====

impl<'a> Entered<'a> {
    /// Exit the `Entered` guard, returning an `Enter` handle that may be used
    /// to re-enter the span, or `None` if the span closed while performing the
    /// exit.
    fn exit(self) -> Option<Enter<'a>> {
        self.inner.subscriber.exit(&self.inner.id);
        if self.inner.closed {
            // Dropping `inner` will allow it to perform the closure if
            // able.
            None
        } else {
            Some(self.inner)
        }
    }
}
