//! Spans represent periods of time in the execution of a program.
//!
//! # Entering a Span
//!
//! A thread of execution is said to _enter_ a span when it begins executing,
//! and _exit_ the span when it switches to another context. Spans may be
//! entered through the [`enter`] and [`in_scope`] methods.
//!
//! The `enter` method enters a span, returning a [guard] that exits the span
//! when dropped
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # use tokio_trace::Level;
//! # fn main() {
//! let my_var: u64 = 5;
//! let my_span = span!(Level::TRACE, "my_span", my_var = &my_var);
//!
//! // `my_span` exists but has not been entered.
//!
//! let _enter = my_span.enter();
//!
//! // Perform some work inside of the context of `my_span`...
//! # }
//!```
//!
//! `in_scope` takes a closure or function pointer and executes it inside the
//! span.
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # use tokio_trace::Level;
//! # fn main() {
//! let my_var: u64 = 5;
//! let my_span = span!(Level::TRACE, "my_span", my_var = &my_var);
//!
//! my_span.in_scope(|| {
//!     // perform some work in the context of `my_span`...
//! });
//!
//! // Perform some work outside of the context of `my_span`...
//!
//! my_span.in_scope(|| {
//!     // Perform some more work in the context of `my_span`.
//! });
//! # }
//! ```
//!
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
//! struct MyFuture {
//!    // data
//!    span: tokio_trace::Span,
//! }
//!
//! impl Future for MyFuture {
//!     type Item = ();
//!     type Error = ();
//!
//!     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//!         self.span.in_scope(|| {
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
//! span, [`exit`] will always be called with that span's ID to notify the
//! subscriber that the span has been exited. When span handles are dropped, the
//! [`drop_span`] method is called with that span's ID. The subscriber may use
//! this to determine whether or not the span will be entered again.
//!
//! If there is only a single handle with the capacity to exit a span, dropping
//! that handle "close" the span, since the capacity to enter it no longer
//! exists. For example:
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # use tokio_trace::Level;
//! # fn main() {
//! {
//!     span!(Level::TRACE, "my_span").in_scope(|| {
//!         // perform some work in the context of `my_span`...
//!     }); // --> Subscriber::exit(my_span)
//!
//!     // The handle to `my_span` only lives inside of this block; when it is
//!     // dropped, the subscriber will be informed via `drop_span`.
//!
//! } // --> Subscriber::drop_span(my_span)
//! # }
//! ```
//!
//! However, if multiple handles exist, the span can still be re-entered even if
//! one or more is dropped. For determining when _all_ handles to a span have
//! been dropped, `Subscriber`s have a [`clone_span`]  method, which is called
//! every time a span handle is cloned. Combined with `drop_span`, this may be
//! used to track the number of handles to a given span — if `drop_span` has
//! been called one more time than the number of calls to `clone_span` for a
//! given ID, then no more handles to the span with that ID exist. The
//! subscriber may then treat it as closed.
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
//! [`clone_span`]: ../subscriber/trait.Subscriber.html#method.clone_span
//! [`drop_span`]: ../subscriber/trait.Subscriber.html#method.drop_span
//! [`exit`]: ../subscriber/trait.Subscriber.html#tymethod.exit
//! [`Subscriber`]: ../subscriber/trait.Subscriber.html
//! [`Attributes`]: struct.Attributes.html
//! [`enter`]: struct.Span.html#method.enter
//! [`in_scope`]: struct.Span.html#method.in_scope
//! [`guard`]: struct.Entered.html
pub use tokio_trace_core::span::{Attributes, Id, Record};

use std::{
    cmp, fmt,
    hash::{Hash, Hasher},
};
use {dispatcher::Dispatch, field, Metadata};

/// Trait implemented by types which have a span `Id`.
pub trait AsId: ::sealed::Sealed {
    /// Returns the `Id` of the span that `self` corresponds to, or `None` if
    /// this corresponds to a disabled span.
    fn as_id(&self) -> Option<&Id>;
}

/// A handle representing a span, with the capability to enter the span if it
/// exists.
///
/// If the span was rejected by the current `Subscriber`'s filter, entering the
/// span will silently do nothing. Thus, the handle can be used in the same
/// manner regardless of whether or not the trace is currently being collected.
#[derive(Clone)]
pub struct Span {
    /// A handle used to enter the span when it is not executing.
    ///
    /// If this is `None`, then the span has either closed or was never enabled.
    inner: Option<Inner>,
    meta: &'static Metadata<'static>,
}

/// A handle representing the capacity to enter a span which is known to exist.
///
/// Unlike `Span`, this type is only constructed for spans which _have_ been
/// enabled by the current filter. This type is primarily used for implementing
/// span handles; users should typically not need to interact with it directly.
#[derive(Debug)]
pub(crate) struct Inner {
    /// The span's ID, as provided by `subscriber`.
    id: Id,

    /// The subscriber that will receive events relating to this span.
    ///
    /// This should be the same subscriber that provided this span with its
    /// `id`.
    subscriber: Dispatch,
}

/// A guard representing a span which has been entered and is currently
/// executing.
///
/// When the guard is dropped, the span will be exited.
///
/// This is returned by the [`Span::enter`] function.
///
/// [`Span::enter`]: ../struct.Span.html#method.enter
#[derive(Debug)]
#[must_use = "once a span has been entered, it should be exited"]
pub struct Entered<'a> {
    span: &'a Span,
}

// ===== impl Span =====

impl Span {
    /// Constructs a new `Span` with the given [metadata] and set of
    /// [field values].
    ///
    /// The new span will be constructed by the currently-active [`Subscriber`],
    /// with the current span as its parent (if one exists).
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [metadata]: ../metadata
    /// [`Subscriber`]: ../subscriber/trait.Subscriber.html
    /// [field values]: ../field/struct.ValueSet.html
    /// [`follows_from`]: ../struct.Span.html#method.follows_from
    #[inline]
    pub fn new(meta: &'static Metadata<'static>, values: &field::ValueSet) -> Span {
        let new_span = Attributes::new(meta, values);
        Self::make(meta, new_span)
    }

    /// Constructs a new `Span` as the root of its own trace tree, with the
    /// given [metadata] and set of [field values].
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [metadata]: ../metadata
    /// [field values]: ../field/struct.ValueSet.html
    /// [`follows_from`]: ../struct.Span.html#method.follows_from
    #[inline]
    pub fn new_root(meta: &'static Metadata<'static>, values: &field::ValueSet) -> Span {
        Self::make(meta, Attributes::new_root(meta, values))
    }

    /// Constructs a new `Span` as child of the given parent span, with the
    /// given [metadata] and set of [field values].
    ///
    /// After the span is constructed, [field values] and/or [`follows_from`]
    /// annotations may be added to it.
    ///
    /// [metadata]: ../metadata
    /// [field values]: ../field/struct.ValueSet.html
    /// [`follows_from`]: ../struct.Span.html#method.follows_from
    pub fn child_of(
        parent: impl Into<Option<Id>>,
        meta: &'static Metadata<'static>,
        values: &field::ValueSet,
    ) -> Span {
        let new_span = match parent.into() {
            Some(parent) => Attributes::child_of(parent, meta, values),
            None => Attributes::new_root(meta, values),
        };
        Self::make(meta, new_span)
    }

    /// Constructs a new disabled span.
    #[inline(always)]
    pub fn new_disabled(meta: &'static Metadata<'static>) -> Span {
        Span { inner: None, meta }
    }

    fn make(meta: &'static Metadata<'static>, new_span: Attributes) -> Span {
        let attrs = &new_span;
        let inner = ::dispatcher::get_default(move |dispatch| {
            let id = dispatch.new_span(attrs);
            Some(Inner::new(id, dispatch))
        });
        let span = Self { inner, meta };
        span.log(format_args!("{}; {}", meta.name(), FmtAttrs(&new_span)));
        span
    }

    /// Enters this span, returning a guard that will exit the span when dropped.
    ///
    /// If this span is enabled by the current subscriber, then this function will
    /// call [`Subscriber::enter`] with the span's [`Id`], and dropping the guard
    /// will call [`Subscriber::exit`]. If the span is disabled, this does nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// #[macro_use] extern crate tokio_trace;
    /// # use tokio_trace::Level;
    /// # fn main() {
    /// let span = span!(Level::INFO, "my_span");
    /// let guard = span.enter();
    ///
    /// // code here is within the span
    ///
    /// drop(guard);
    ///
    /// // code here is no longer within the span
    ///
    /// # }
    /// ```
    ///
    /// Guards need not be explicitly dropped:
    ///
    /// ```
    /// #[macro_use] extern crate tokio_trace;
    /// # fn main() {
    /// fn my_function() -> String {
    ///     // enter a span for the duration of this function.
    ///     let span = trace_span!("my_function");
    ///     let _enter = span.enter();
    ///
    ///     // anything happening in functions we call is still inside the span...
    ///     my_other_function();
    ///
    ///     // returning from the function drops the guard, exiting the span.
    ///     return "Hello world".to_owned();
    /// }
    ///
    /// fn my_other_function() {
    ///     // ...
    /// }
    /// # }
    /// ```
    ///
    /// Sub-scopes may be created to limit the duration for which the span is
    /// entered:
    ///
    /// ```
    /// #[macro_use] extern crate tokio_trace;
    /// # fn main() {
    /// let span = info_span!("my_great_span");
    ///
    /// {
    ///     let _enter = span.enter();
    ///
    ///     // this event occurs inside the span.
    ///     info!("i'm in the span!");
    ///
    ///     // exiting the scope drops the guard, exiting the span.
    /// }
    ///
    /// // this event is not inside the span.
    /// info!("i'm outside the span!")
    /// # }
    /// ```
    ///
    /// [`Subscriber::enter`]: ../subscriber/trait.Subscriber.html#method.enter
    /// [`Subscriber::exit`]: ../subscriber/trait.Subscriber.html#method.exit
    /// [`Id`]: ../struct.Id.html
    pub fn enter<'a>(&'a self) -> Entered<'a> {
        if let Some(ref inner) = self.inner.as_ref() {
            inner.subscriber.enter(&inner.id);
        }
        self.log(format_args!("-> {}", self.meta.name));
        Entered { span: self }
    }

    /// Executes the given function in the context of this span.
    ///
    /// If this span is enabled, then this function enters the span, invokes `f`
    /// and then exits the span. If the span is disabled, `f` will still be
    /// invoked, but in the context of the currently-executing span (if there is
    /// one).
    ///
    /// Returns the result of evaluating `f`.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate tokio_trace;
    /// # use tokio_trace::Level;
    /// # fn main() {
    /// let my_span = span!(Level::TRACE, "my_span");
    ///
    /// my_span.in_scope(|| {
    ///     // this event occurs within the span.
    ///     trace!("i'm in the span!");
    /// });
    ///
    /// // this event occurs outside the span.
    /// trace!("i'm not in the span!");
    /// # }
    /// ```
    ///
    /// Calling a function and returning the result:
    /// ```
    /// # #[macro_use] extern crate tokio_trace;
    /// # use tokio_trace::Level;
    /// fn hello_world() -> String {
    ///     "Hello world!".to_owned()
    /// }
    ///
    /// # fn main() {
    /// let span = info_span!("hello_world");
    /// // the span will be entered for the duration of the call to
    /// // `hello_world`.
    /// let a_string = span.in_scope(hello_world);
    /// # }
    ///
    pub fn in_scope<F: FnOnce() -> T, T>(&self, f: F) -> T {
        let _enter = self.enter();
        f()
    }

    /// Returns a [`Field`](../field/struct.Field.html) for the field with the
    /// given `name`, if one exists,
    pub fn field<Q: ?Sized>(&self, field: &Q) -> Option<field::Field>
    where
        Q: field::AsField,
    {
        self.metadata().and_then(|meta| field.as_field(meta))
    }

    /// Returns true if this `Span` has a field for the given
    /// [`Field`](../field/struct.Field.html) or field name.
    #[inline]
    pub fn has_field<Q: ?Sized>(&self, field: &Q) -> bool
    where
        Q: field::AsField,
    {
        self.field(field).is_some()
    }

    /// Visits that the field described by `field` has the value `value`.
    pub fn record<Q: ?Sized, V>(&self, field: &Q, value: &V) -> &Self
    where
        Q: field::AsField,
        V: field::Value,
    {
        if let Some(field) = field.as_field(self.meta) {
            self.record_all(
                &self
                    .meta
                    .fields()
                    .value_set(&[(&field, Some(value as &field::Value))]),
            );
        }

        self
    }

    /// Visit all the fields in the span
    pub fn record_all(&self, values: &field::ValueSet) -> &Self {
        let record = Record::new(values);
        if let Some(ref inner) = self.inner {
            inner.record(&record);
        }
        self.log(format_args!("{}; {}", self.meta.name(), FmtValues(&record)));
        self
    }

    /// Returns `true` if this span was disabled by the subscriber and does not
    /// exist.
    #[inline]
    pub fn is_disabled(&self) -> bool {
        self.inner.is_none()
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
    pub fn follows_from(&self, from: impl for<'a> Into<Option<&'a Id>>) -> &Self {
        if let Some(ref inner) = self.inner {
            if let Some(from) = from.into() {
                inner.follows_from(from);
            }
        }
        self
    }

    /// Returns this span's `Id`, if it is enabled.
    pub fn id(&self) -> Option<Id> {
        self.inner.as_ref().map(Inner::id)
    }

    /// Returns this span's `Metadata`, if it is enabled.
    pub fn metadata(&self) -> Option<&'static Metadata<'static>> {
        if self.inner.is_some() {
            Some(self.meta)
        } else {
            None
        }
    }

    #[cfg(feature = "log")]
    #[inline]
    fn log(&self, message: fmt::Arguments) {
        use log;
        let logger = log::logger();
        let log_meta = log::Metadata::builder()
            .level(level_to_log!(self.meta.level))
            .target(self.meta.target)
            .build();
        if logger.enabled(&log_meta) {
            logger.log(
                &log::Record::builder()
                    .metadata(log_meta)
                    .module_path(self.meta.module_path)
                    .file(self.meta.file)
                    .line(self.meta.line)
                    .args(message)
                    .build(),
            );
        }
    }

    #[cfg(not(feature = "log"))]
    #[inline]
    fn log(&self, _: fmt::Arguments) {}
}

impl cmp::PartialEq for Span {
    fn eq(&self, other: &Self) -> bool {
        self.meta.callsite() == other.meta.callsite() && self.inner == other.inner
    }
}

impl Hash for Span {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.inner.hash(hasher);
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut span = f.debug_struct("Span");
        span.field("name", &self.meta.name())
            .field("level", &self.meta.level())
            .field("target", &self.meta.target());

        if let Some(ref inner) = self.inner {
            span.field("id", &inner.id());
        } else {
            span.field("disabled", &true);
        }

        if let Some(ref path) = self.meta.module_path() {
            span.field("module_path", &path);
        }

        if let Some(ref line) = self.meta.line() {
            span.field("line", &line);
        }

        if let Some(ref file) = self.meta.file() {
            span.field("file", &file);
        }

        span.finish()
    }
}

impl<'a> Into<Option<&'a Id>> for &'a Span {
    fn into(self) -> Option<&'a Id> {
        self.inner.as_ref().map(|inner| &inner.id)
    }
}

impl<'a> Into<Option<Id>> for &'a Span {
    fn into(self) -> Option<Id> {
        self.inner.as_ref().map(Inner::id)
    }
}

impl Into<Option<Id>> for Span {
    fn into(self) -> Option<Id> {
        self.inner.as_ref().map(Inner::id)
    }
}

// ===== impl Inner =====

impl Inner {
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

    fn record(&self, values: &Record) {
        self.subscriber.record(&self.id, values)
    }

    fn new(id: Id, subscriber: &Dispatch) -> Self {
        Inner {
            id,
            subscriber: subscriber.clone(),
        }
    }
}

impl cmp::PartialEq for Inner {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Hash for Inner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.subscriber.drop_span(self.id.clone());
    }
}

impl Clone for Inner {
    fn clone(&self) -> Self {
        Inner {
            id: self.subscriber.clone_span(&self.id),
            subscriber: self.subscriber.clone(),
        }
    }
}

// ===== impl Entered =====

impl<'a> Drop for Entered<'a> {
    #[inline]
    fn drop(&mut self) {
        // Dropping the guard exits the span.
        //
        // Running this behaviour on drop rather than with an explicit function
        // call means that spans may still be exited when unwinding.
        if let Some(inner) = self.span.inner.as_ref() {
            inner.subscriber.exit(&inner.id);
        }
        self.span.log(format_args!("<- {}", self.span.meta.name));
    }
}

struct FmtValues<'a>(&'a Record<'a>);

impl<'a> fmt::Display for FmtValues<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut res = Ok(());
        self.0.record(&mut |k: &field::Field, v: &fmt::Debug| {
            res = write!(f, "{}={:?} ", k, v);
        });
        res
    }
}

struct FmtAttrs<'a>(&'a Attributes<'a>);

impl<'a> fmt::Display for FmtAttrs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut res = Ok(());
        self.0.record(&mut |k: &field::Field, v: &fmt::Debug| {
            res = write!(f, "{}={:?} ", k, v);
        });
        res
    }
}

#[cfg(test)]
mod test {
    use super::*;

    trait AssertSend: Send {}
    impl AssertSend for Span {}

    trait AssertSync: Sync {}
    impl AssertSync for Span {}
}
