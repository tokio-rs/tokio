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
//! — if `drop_span` has been called one more time than the number of calls to
//! `clone_span` for a given ID, then no more handles to the span with that ID
//! exist. The subscriber may then treat it as closed.
//!
//! # Accessing a Span's Attributes
//!
//! The [`Attributes`] type represents a *non-entering* reference to a `Span`'s data
//! — a set of key-value pairs (known as _fields_), a creation timestamp,
//! a reference to the span's parent in the trace tree, and metadata describing
//! the source code location where the span was created. This data is provided
//! to the [`Subscriber`] when the span is created; it may then choose to cache
//! the data for future use, record it in some manner, or discard it completely.
//!
//! [`Subscriber`]: ::Subscriber
pub use tokio_trace_core::span::{Attributes, Id, Record};

use std::{
    borrow::Borrow,
    cmp, fmt,
    hash::{Hash, Hasher},
};
use {
    dispatcher::{self, Dispatch},
    field, Metadata,
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
    inner: Option<Inner<'a>>,

    /// Set to `true` when the span closes.
    ///
    /// This allows us to distinguish if `inner` is `None` because the span was
    /// never enabled (and thus the inner state was never created), or if the
    /// previously entered, but it is now closed.
    is_closed: bool,
}

/// A handle representing the capacity to enter a span which is known to exist.
///
/// Unlike `Span`, this type is only constructed for spans which _have_ been
/// enabled by the current filter. This type is primarily used for implementing
/// span handles; users should typically not need to interact with it directly.
#[derive(Debug)]
pub(crate) struct Inner<'a> {
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
    inner: Inner<'a>,
}

// ===== impl Span =====

impl<'a> Span<'a> {
    /// Constructs a new `Span` with the given [metadata] and set of [field
    /// values].
    ///
    /// The new span will be constructed by the currently-active [`Subscriber`],
    /// with the current span as its parent (if one exists).
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [metadata]: ::metadata::Metadata
    /// [`Subscriber`]: ::subscriber::Subscriber
    /// [field values]: ::field::ValueSet
    /// [`follows_from`]: ::span::Span::follows_from
    #[inline]
    pub fn new(meta: &'a Metadata<'a>, values: &field::ValueSet) -> Span<'a> {
        let new_span = Attributes::new(meta, values);
        Self::make(meta, new_span)
    }

    /// Constructs a new `Span` as the root of its own trace tree, with the
    /// given [metadata] and set of [field values].
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [metadata]: ::metadata::Metadata
    /// [field values]: ::field::ValueSet
    /// [`follows_from`]: ::span::Span::follows_from
    #[inline]
    pub fn new_root(meta: &'a Metadata<'a>, values: &field::ValueSet) -> Span<'a> {
        Self::make(meta, Attributes::new_root(meta, values))
    }

    /// Constructs a new `Span` as child of the given parent span, with the
    /// given [metadata] and set of [field values].
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [metadata]: ::metadata::Metadata
    /// [field values]: ::field::ValueSet
    /// [`follows_from`]: ::span::Span::follows_from
    pub fn child_of<I>(parent: I, meta: &'a Metadata<'a>, values: &field::ValueSet) -> Span<'a>
    where
        I: Into<Option<Id>>,
    {
        let new_span = match parent.into() {
            Some(parent) => Attributes::child_of(parent, meta, values),
            None => Attributes::new_root(meta, values),
        };
        Self::make(meta, new_span)
    }

    /// Constructs a new disabled span.
    #[inline(always)]
    pub fn new_disabled() -> Span<'a> {
        Span {
            inner: None,
            is_closed: false,
        }
    }

    #[inline(always)]
    fn make(meta: &'a Metadata<'a>, new_span: Attributes) -> Span<'a> {
        let inner = dispatcher::with(move |dispatch| {
            let id = dispatch.new_span(&new_span);
            Some(Inner::new(id, dispatch, meta))
        });
        Self {
            inner,
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
    pub fn field<Q>(&self, name: &Q) -> Option<field::Field>
    where
        Q: Borrow<str>,
    {
        self.inner
            .as_ref()
            .and_then(|inner| inner.meta.fields().field(name))
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

    /// Visits that the field described by `field` has the value `value`.
    pub fn record<Q: ?Sized, V>(&mut self, field: &Q, value: &V) -> &mut Self
    where
        Q: field::AsField,
        V: field::Value,
    {
        if let Some(ref mut inner) = self.inner {
            let meta = inner.metadata();
            if let Some(field) = field.as_field(meta) {
                inner.record(
                    &meta
                        .fields()
                        .value_set(&[(&field, Some(value as &field::Value))]),
                )
            }
        }
        self
    }

    /// Visit all the fields in the span
    pub fn record_all(&mut self, values: &field::ValueSet) -> &mut Self {
        if let Some(ref mut inner) = self.inner {
            inner.record(&values);
        }
        self
    }

    /// Closes this span handle, dropping its internal state.
    ///
    /// Once this function has been called, subsequent calls to `enter` on this
    /// handle will no longer enter the span. If this is the final handle with
    /// the potential to enter that span, the subscriber may consider the span to
    /// have ended.
    pub fn close(&mut self) {
        if let Some(mut inner) = self.inner.take() {
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
    #[inline]
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
    pub fn follows_from(&self, from: &Id) -> &Self {
        if let Some(ref inner) = self.inner {
            inner.follows_from(from);
        }
        self
    }

    /// Returns this span's `Id`, if it is enabled.
    pub fn id(&self) -> Option<Id> {
        self.inner.as_ref().map(Inner::id)
    }

    /// Returns this span's `Metadata`, if it is enabled.
    pub fn metadata(&self) -> Option<&'a Metadata<'a>> {
        self.inner.as_ref().map(Inner::metadata)
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

impl<'a> Into<Option<Id>> for &'a Span<'a> {
    fn into(self) -> Option<Id> {
        self.id()
    }
}

// ===== impl Inner =====

impl<'a> Inner<'a> {
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
    fn follows_from(&self, from: &Id) {
        self.subscriber.record_follows_from(&self.id, &from)
    }

    /// Returns the span's ID.
    fn id(&self) -> Id {
        self.id.clone()
    }

    /// Returns the span's metadata.
    fn metadata(&self) -> &'a Metadata<'a> {
        self.meta
    }

    fn record(&mut self, values: &field::ValueSet) {
        if values.callsite() == self.meta.callsite() {
            self.subscriber.record(&self.id, &Record::new(values))
        }
    }

    fn new(id: Id, subscriber: &Dispatch, meta: &'a Metadata<'a>) -> Self {
        Inner {
            id,
            subscriber: subscriber.clone(),
            closed: false,
            meta,
        }
    }
}

impl<'a> cmp::PartialEq for Inner<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> Hash for Inner<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<'a> Drop for Inner<'a> {
    fn drop(&mut self) {
        self.subscriber.drop_span(self.id.clone());
    }
}

impl<'a> Clone for Inner<'a> {
    fn clone(&self) -> Self {
        Inner {
            id: self.subscriber.clone_span(&self.id),
            subscriber: self.subscriber.clone(),
            closed: self.closed,
            meta: self.meta,
        }
    }
}

// ===== impl Entered =====

impl<'a> Entered<'a> {
    /// Exit the `Entered` guard, returning an `Inner` handle that may be used
    /// to re-enter the span, or `None` if the span closed while performing the
    /// exit.
    fn exit(self) -> Option<Inner<'a>> {
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
