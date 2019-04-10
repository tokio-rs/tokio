//! Subscribers collect and record trace data.
use {span, Event, Metadata};

use std::{
    any::{Any, TypeId},
    ptr,
};

/// Trait representing the functions required to collect trace data.
///
/// Crates that provide implementations of methods for collecting or recording
/// trace data should implement the `Subscriber` interface. This trait is
/// intended to represent fundamental primitives for collecting trace events and
/// spans — other libraries may offer utility functions and types to make
/// subscriber implementations more modular or improve the ergonomics of writing
/// subscribers.
///
/// A subscriber is responsible for the following:
/// - Registering new spans as they are created, and providing them with span
///   IDs. Implicitly, this means the subscriber may determine the strategy for
///   determining span equality.
/// - Recording the attachment of field values and follows-from annotations to
///   spans.
/// - Filtering spans and events, and determining when those filters must be
///   invalidated.
/// - Observing spans as they are entered, exited, and closed, and events as
///   they occur.
///
/// When a span is entered or exited, the subscriber is provided only with the
/// [ID] with which it tagged that span when it was created. This means
/// that it is up to the subscriber to determine whether and how span _data_ —
/// the fields and metadata describing the span — should be stored. The
/// [`new_span`] function is called when a new span is created, and at that
/// point, the subscriber _may_ choose to store the associated data if it will
/// be referenced again. However, if the data has already been recorded and will
/// not be needed by the implementations of `enter` and `exit`, the subscriber
/// may freely discard that data without allocating space to store it.
///
/// [ID]: ../span/struct.Id.html
/// [`new_span`]: trait.Subscriber.html#method.new_span
pub trait Subscriber: 'static {
    // === Span registry methods ==============================================

    /// Registers a new callsite with this subscriber, returning whether or not
    /// the subscriber is interested in being notified about the callsite.
    ///
    /// By default, this function assumes that the subscriber's [filter]
    /// represents an unchanging view of its interest in the callsite. However,
    /// if this is not the case, subscribers may override this function to
    /// indicate different interests, or to implement behaviour that should run
    /// once for every callsite.
    ///
    /// This function is guaranteed to be called at least once per callsite on
    /// every active subscriber. The subscriber may store the keys to fields it
    /// cares about in order to reduce the cost of accessing fields by name,
    /// preallocate storage for that callsite, or perform any other actions it
    /// wishes to perform once for each callsite.
    ///
    /// The subscriber should then return an [`Interest`], indicating
    /// whether it is interested in being notified about that callsite in the
    /// future. This may be `Always` indicating that the subscriber always
    /// wishes to be notified about the callsite, and its filter need not be
    /// re-evaluated; `Sometimes`, indicating that the subscriber may sometimes
    /// care about the callsite but not always (such as when sampling), or
    /// `Never`, indicating that the subscriber never wishes to be notified about
    /// that callsite. If all active subscribers return `Never`, a callsite will
    /// never be enabled unless a new subscriber expresses interest in it.
    ///
    /// `Subscriber`s which require their filters to be run every time an event
    /// occurs or a span is entered/exited should return `Interest::sometimes`.
    /// If a subscriber returns `Interest::sometimes`, then its' [`enabled`] method
    /// will be called every time an event or span is created from that callsite.
    ///
    /// For example, suppose a sampling subscriber is implemented by
    /// incrementing a counter every time `enabled` is called and only returning
    /// `true` when the counter is divisible by a specified sampling rate. If
    /// that subscriber returns `Interest::always` from `register_callsite`, then
    /// the filter will not be re-evaluated once it has been applied to a given
    /// set of metadata. Thus, the counter will not be incremented, and the span
    /// or event that correspands to the metadata will never be `enabled`.
    ///
    /// `Subscriber`s that need to change their filters occasionally should call
    /// [`rebuild_interest_cache`] to re-evaluate `register_callsite` for all
    /// callsites.
    ///
    /// Similarly, if a `Subscriber` has a filtering strategy that can be
    /// changed dynamically at runtime, it would need to re-evaluate that filter
    /// if the cached results have changed.
    ///
    /// A subscriber which manages fanout to multiple other subscribers
    /// should proxy this decision to all of its child subscribers,
    /// returning `Interest::never` only if _all_ such children return
    /// `Interest::never`. If the set of subscribers to which spans are
    /// broadcast may change dynamically, the subscriber should also never
    /// return `Interest::Never`, as a new subscriber may be added that _is_
    /// interested.
    ///
    /// # Notes
    /// This function may be called again when a new subscriber is created or
    /// when the registry is invalidated.
    ///
    /// If a subscriber returns `Interest::never` for a particular callsite, it
    /// _may_ still see spans and events originating from that callsite, if
    /// another subscriber expressed interest in it.
    ///
    /// [filter]: #method.enabled
    /// [metadata]: ../metadata/struct.Metadata.html
    /// [`Interest`]: struct.Interest.html
    /// [`enabled`]: #method.enabled
    /// [`rebuild_interest_cache`]: ../callsite/fn.rebuild_interest_cache.html
    fn register_callsite(&self, metadata: &Metadata) -> Interest {
        match self.enabled(metadata) {
            true => Interest::always(),
            false => Interest::never(),
        }
    }

    /// Returns true if a span or event with the specified [metadata] would be
    /// recorded.
    ///
    /// By default, it is assumed that this filter needs only be evaluated once
    /// for each callsite, so it is called by [`register_callsite`] when each
    /// callsite is registered. The result is used to determine if the subscriber
    /// is always [interested] or never interested in that callsite. This is intended
    /// primarily as an optimization, so that expensive filters (such as those
    /// involving string search, et cetera) need not be re-evaluated.
    ///
    /// However, if the subscriber's interest in a particular span or event may
    /// change, or depends on contexts only determined dynamically at runtime,
    /// then the `register_callsite` method should be overridden to return
    /// [`Interest::sometimes`]. In that case, this function will be called every
    /// time that span or event occurs.
    ///
    /// [metadata]: ../metadata/struct.Metadata.html
    /// [interested]: struct.Interest.html
    /// [`Interest::sometimes`]: struct.Interest.html#method.sometimes
    /// [`register_callsite`]: #method.register_callsite
    fn enabled(&self, metadata: &Metadata) -> bool;

    /// Visit the construction of a new span, returning a new [span ID] for the
    /// span being constructed.
    ///
    /// The provided [`Attributes`] contains any field values that were provided
    /// when the span was created. The subscriber may pass a [visitor] to the
    /// `Attributes`' [`record` method] to record these values.
    ///
    /// IDs are used to uniquely identify spans and events within the context of a
    /// subscriber, so span equality will be based on the returned ID. Thus, if
    /// the subscriber wishes for all spans with the same metadata to be
    /// considered equal, it should return the same ID every time it is given a
    /// particular set of metadata. Similarly, if it wishes for two separate
    /// instances of a span with the same metadata to *not* be equal, it should
    /// return a distinct ID every time this function is called, regardless of
    /// the metadata.
    ///
    /// Note that the subscriber is free to assign span IDs based on whatever
    /// scheme it sees fit. Any guarantees about uniqueness, ordering, or ID
    /// reuse are left up to the subscriber implementation to determine.
    ///
    /// [span ID]: ../span/struct.Id.html
    /// [`Attributes`]: ../span/struct.Attributes.html
    /// [visitor]: ../field/trait.Visit.html
    /// [`record` method]: ../span/struct.Attributes.html#method.record
    fn new_span(&self, span: &span::Attributes) -> span::Id;

    // === Notification methods ===============================================

    /// Record a set of values on a span.
    ///
    /// This method will be invoked when value is recorded on a span.
    /// Recording multiple values for the same field is possible,
    /// but the actual behaviour is defined by the subscriber implementation.
    ///
    /// Keep in mind that a span might not provide a value
    /// for each field it declares.
    ///
    /// The subscriber is expected to provide a [visitor] to the `Record`'s
    /// [`record` method] in order to record the added values.
    ///
    /// # Example
    ///  "foo = 3" will be recorded when [`record`] is called on the
    /// `Attributes` passed to `new_span`.
    /// Since values are not provided for the `bar` and `baz` fields,
    /// the span's `Metadata` will indicate that it _has_ those fields,
    /// but values for them won't be recorded at this time.
    ///
    /// ```rust,ignore
    /// #[macro_use]
    /// extern crate tokio_trace;
    ///
    /// let mut span = span!("my_span", foo = 3, bar, baz);
    ///
    /// // `Subscriber::record` will be called with a `Record`
    /// // containing "bar = false"
    /// span.record("bar", &false);
    ///
    /// // `Subscriber::record` will be called with a `Record`
    /// // containing "baz = "a string""
    /// span.record("baz", &"a string");
    /// ```
    ///
    /// [visitor]: ../field/trait.Visit.html
    /// [`record`]: ../span/struct.Attributes.html#method.record
    /// [`record` method]: ../span/struct.Record.html#method.record
    fn record(&self, span: &span::Id, values: &span::Record);

    /// Adds an indication that `span` follows from the span with the id
    /// `follows`.
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
    /// If the subscriber has spans corresponding to the given IDs, it should
    /// record this relationship in whatever way it deems necessary. Otherwise,
    /// if one or both of the given span IDs do not correspond to spans that the
    /// subscriber knows about, or if a cyclical relationship would be created
    /// (i.e., some span _a_ which proceeds some other span _b_ may not also
    /// follow from _b_), it may silently do nothing.
    fn record_follows_from(&self, span: &span::Id, follows: &span::Id);

    /// Records that an [`Event`] has occurred.
    ///
    /// This method will be invoked when an Event is constructed by
    /// the `Event`'s [`dispatch` method]. For example, this happens internally
    /// when an event macro from `tokio-trace` is called.
    ///
    /// The key difference between this method and `record` is that `record` is
    /// called when a value is recorded for a field defined by a span,
    /// while `event` is called when a new event occurs.
    ///
    /// The provided `Event` struct contains any field values attached to the
    /// event. The subscriber may pass a [visitor] to the `Event`'s
    /// [`record` method] to record these values.
    ///
    /// [`Event`]: ../event/struct.Event.html
    /// [visitor]: ../field/trait.Visit.html
    /// [`record` method]: ../event/struct.Event.html#method.record
    /// [`dispatch` method]: ../event/struct.Event.html#method.dispatch
    fn event(&self, event: &Event);

    /// Records that a span has been entered.
    ///
    /// When entering a span, this method is called to notify the subscriber
    /// that the span has been entered. The subscriber is provided with the
    /// [span ID] of the entered span, and should update any internal state
    /// tracking the current span accordingly.
    ///
    /// [span ID]: ../span/struct.Id.html
    fn enter(&self, span: &span::Id);

    /// Records that a span has been exited.
    ///
    /// When entering a span, this method is called to notify the subscriber
    /// that the span has been exited. The subscriber is provided with the
    /// [span ID] of the exited span, and should update any internal state
    /// tracking the current span accordingly.
    ///
    /// Exiting a span does not imply that the span will not be re-entered.
    ///
    /// [span ID]: ../span/struct.Id.html
    fn exit(&self, span: &span::Id);

    /// Notifies the subscriber that a [span ID] has been cloned.
    ///
    /// This function is guaranteed to only be called with span IDs that were
    /// returned by this subscriber's `new_span` function.
    ///
    /// Note that the default implementation of this function this is just the
    /// identity function, passing through the identifier. However, it can be
    /// used in conjunction with [`drop_span`] to track the number of handles
    /// capable of `enter`ing a span. When all the handles have been dropped
    /// (i.e., `drop_span` has been called one more time than `clone_span` for a
    /// given ID), the subscriber may assume that the span will not be entered
    /// again. It is then free to deallocate storage for data associated with
    /// that span, write data from that span to IO, and so on.
    ///
    /// For more unsafe situations, however, if `id` is itself a pointer of some
    /// kind this can be used as a hook to "clone" the pointer, depending on
    /// what that means for the specified pointer.
    ///
    /// [span ID]: ../span/struct.Id.html
    /// [`drop_span`]: trait.Subscriber.html#method.drop_span
    fn clone_span(&self, id: &span::Id) -> span::Id {
        id.clone()
    }

    /// Notifies the subscriber that a [span ID] has been dropped.
    ///
    /// This function is guaranteed to only be called with span IDs that were
    /// returned by this subscriber's `new_span` function.
    ///
    /// It's guaranteed that if this function has been called once more than the
    /// number of times `clone_span` was called with the same `id`, then no more
    /// spans using that `id` exist. This means that it can be used in
    /// conjunction with [`clone_span`] to track the number of handles
    /// capable of `enter`ing a span. When all the handles have been dropped
    /// (i.e., `drop_span` has been called one more time than `clone_span` for a
    /// given ID), the subscriber may assume that the span will not be entered
    /// again. It is then free to deallocate storage for data associated with
    /// that span, write data from that span to IO, and so on.
    ///
    /// **Note**: since this function is called when spans are dropped,
    /// implementations should ensure that they are unwind-safe. Panicking from
    /// inside of a `drop_span` function may cause a double panic, if the span
    /// was dropped due to a thread unwinding.
    ///
    /// [span ID]: ../span/struct.Id.html
    /// [`clone_span`]: trait.Subscriber.html#method.clone_span
    fn drop_span(&self, id: span::Id) {
        let _ = id;
    }

    // === Downcasting methods ================================================

    /// If `self` is the same type as the provided `TypeId`, returns an untyped
    /// `*const` pointer to that type. Otherwise, returns `None`.
    ///
    /// If you wish to downcast a `Subscriber`, it is strongly advised to use
    /// the safe API provided by [`downcast_ref`] instead.
    ///
    /// This API is required for `downcast_raw` to be a trait method; a method
    /// signature like [`downcast_ref`] (with a generic type parameter) is not
    /// object-safe, and thus cannot be a trait method for `Subscriber`. This
    /// means that if we only exposed `downcast_ref`, `Subscriber`
    /// implementations could not override the downcasting behavior
    ///
    /// This method may be overridden by "fan out" or "chained" subscriber
    /// implementations which consist of multiple composed types. Such
    /// subscribers might allow `downcast_raw` by returning references to those
    /// component if they contain components with the given `TypeId`.
    ///
    /// # Safety
    ///
    /// The [`downcast_ref`] method expects that the pointer returned by
    /// `downcast_raw` is non-null and points to a valid instance of the type
    /// with the provided `TypeId`. Failure to ensure this will result in
    /// undefined behaviour, so implementing `downcast_raw` is unsafe.
    ///
    /// [`downcast_ref`]: #method.downcast_ref
    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        if id == TypeId::of::<Self>() {
            Some(self as *const Self as *const ())
        } else {
            None
        }
    }
}

impl Subscriber {
    /// Returns `true` if this `Subscriber` is the same type as `T`.
    pub fn is<T: Any>(&self) -> bool {
        self.downcast_ref::<T>().is_some()
    }

    /// Returns some reference to this `Subscriber` value if it is of type `T`,
    /// or `None` if it isn't.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        unsafe {
            let raw = self.downcast_raw(TypeId::of::<T>())?;
            if raw == ptr::null() {
                None
            } else {
                Some(&*(raw as *const _))
            }
        }
    }
}

/// Indicates a [`Subscriber`]'s interest in a particular callsite.
///
/// `Subscriber`s return an `Interest` from their [`register_callsite`] methods
/// in order to determine whether that span should be enabled or disabled.
///
/// [`Subscriber`] trait.Subscriber.html
/// [clone_span]: trait.Subscriber.html#method.register_callsite
#[derive(Clone, Debug)]
pub struct Interest(InterestKind);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum InterestKind {
    Never = 0,
    Sometimes = 1,
    Always = 2,
}

impl Interest {
    /// Returns an `Interest` indicating that the subscriber is never interested
    /// in being notified about a callsite.
    ///
    /// If all active subscribers are `never()` interested in a callsite, it will
    /// be completely disabled unless a new subscriber becomes active.
    #[inline]
    pub fn never() -> Self {
        Interest(InterestKind::Never)
    }

    /// Returns an `Interest` indicating  the subscriber is sometimes interested
    /// in being notified about a callsite.
    ///
    /// If all active subscribers are `sometimes` or `never` interested in a
    /// callsite, the currently active subscriber will be asked to filter that
    /// callsite every time it creates a span. This will be the case until a new
    /// subscriber expresses that it is `always` interested in the callsite.
    #[inline]
    pub fn sometimes() -> Self {
        Interest(InterestKind::Sometimes)
    }

    /// Returns an `Interest` indicating  the subscriber is always interested in
    /// being notified about a callsite.
    ///
    /// If any subscriber expresses that it is `always()` interested in a given
    /// callsite, then the callsite will always be enabled.
    #[inline]
    pub fn always() -> Self {
        Interest(InterestKind::Always)
    }

    /// Returns `true` if the subscriber is never interested in being notified
    /// about this callsite.
    #[inline]
    pub fn is_never(&self) -> bool {
        match self.0 {
            InterestKind::Never => true,
            _ => false,
        }
    }

    /// Returns `true` if the subscriber is sometimes interested in being notified
    /// about this callsite.
    #[inline]
    pub fn is_sometimes(&self) -> bool {
        match self.0 {
            InterestKind::Sometimes => true,
            _ => false,
        }
    }

    /// Returns `true` if the subscriber is always interested in being notified
    /// about this callsite.
    #[inline]
    pub fn is_always(&self) -> bool {
        match self.0 {
            InterestKind::Always => true,
            _ => false,
        }
    }

    /// Returns the common interest between these two Interests.
    ///
    /// The common interest is defined as the least restrictive, so if one
    /// interest is `never` and the other is `always` the common interest is
    /// `always`.
    pub(crate) fn and(self, rhs: Interest) -> Self {
        match rhs.0 {
            // If the added interest is `never()`, don't change anything —
            // either a different subscriber added a higher interest, which we
            // want to preserve, or the interest is 0 anyway (as it's
            // initialized to 0).
            InterestKind::Never => self,
            // If the interest is `sometimes()`, that overwrites a `never()`
            // interest, but doesn't downgrade an `always()` interest.
            InterestKind::Sometimes if self.0 == InterestKind::Never => rhs,
            // If the interest is `always()`, we overwrite the current interest,
            // as always() is the highest interest level and should take
            // precedent.
            InterestKind::Always => rhs,
            _ => self,
        }
    }
}
