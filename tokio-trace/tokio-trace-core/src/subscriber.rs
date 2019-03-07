//! Subscribers collect and record trace data.
use {span, Event, Metadata};

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
/// - Visiting the attachment of field values and follows-from annotations to
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
/// [ID]: ::span::Span
/// [`new_span`]: ::Span::new_span
pub trait Subscriber: 'static {
    // === Span registry methods ==============================================

    /// Registers a new callsite with this subscriber, returning whether or not
    /// the subscriber is interested in being notified about the callsite.
    ///
    /// By default, this function assumes that the subscriber's filter
    /// represents an unchanging view of its interest in the callsite. However,
    /// if this is not the case, subscribers may override this function to
    /// indicate different interests, or to implement behaviour that should run
    /// once for every callsite.
    ///
    /// This function is guaranteed to be called exactly once per callsite on
    /// every active subscriber. The subscriber may store the keys to fields it
    /// cares in order to reduce the cost of accessing fields by name,
    /// preallocate storage for that callsite, or perform any other actions it
    /// wishes to perform once for each callsite.
    ///
    /// The subscriber should then return an [`Interest`](Interest), indicating
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
    /// occurs or a span is entered/exited should return `Interest::Sometimes`.
    ///
    /// For example, suppose a sampling subscriber is implemented by
    /// incrementing a counter every time `enabled` is called and only returning
    /// `true` when the counter is divisible by a specified sampling rate. If
    /// that subscriber returns `Interest::Always` from `register_callsite`, then
    /// the filter will not be re-evaluated once it has been applied to a given
    /// set of metadata. Thus, the counter will not be incremented, and the span
    /// or event that correspands to the metadata will never be `enabled`.
    ///
    /// Similarly, if a `Subscriber` has a filtering strategy that can be
    /// changed dynamically at runtime, it would need to re-evaluate that filter
    /// if the cached results have changed.
    ///
    /// A subscriber which manages fanout to multiple other subscribers
    /// should proxy this decision to all of its child subscribers,
    /// returning `Interest::Never` only if _all_ such children return
    /// `Interest::Never`. If the set of subscribers to which spans are
    /// broadcast may change dynamically, the subscriber should also never
    /// return `Interest::Never`, as a new subscriber may be added that _is_
    /// interested.
    ///
    /// **Note**: If a subscriber returns `Interest::Never` for a particular
    /// callsite, it _may_ still see spans and events originating from that
    /// callsite, if another subscriber expressed interest in it.
    ///
    /// [metadata]: ::Metadata
    /// [`enabled`]: ::Subscriber::enabled
    fn register_callsite(&self, metadata: &Metadata) -> Interest {
        match self.enabled(metadata) {
            true => Interest::always(),
            false => Interest::never(),
        }
    }

    /// Returns true if a span with the specified [metadata] would be
    /// recorded.
    ///
    /// This is used by the dispatcher to avoid allocating for span construction
    /// if the span would be discarded anyway.
    ///
    /// [metadata]: ::Metadata
    fn enabled(&self, metadata: &Metadata) -> bool;

    /// Visit the construction of a new span, returning a new [span ID] for the
    /// span being constructed.
    ///
    /// The provided `ValueSet` contains any field values that were provided
    /// when the span was created. The subscriber may pass a [visitor] to the
    /// `ValueSet`'s [`record` method] to record these values.
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
    /// [span ID]: ../span/struct.Id.html
    /// [visitor]: ::field::Visit
    /// [`record` method]: ::field::ValueSet::record
    fn new_span(&self, span: &span::Attributes) -> span::Id;

    // === Notification methods ===============================================

    /// Visit a set of values on a span.
    ///
    /// The subscriber is expected to provide a [visitor] to the `Record`'s
    /// [`record` method] in order to record the added values.
    ///
    /// [visitor]: ::field::Visit
    /// [`record` method]: ::span::Record::record
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

    /// Visits that an [`Event`] has occurred.
    ///
    /// The provided `Event` struct contains any field values attached to the
    /// event. The subscriber may pass a [visitor] to the `Event`'s
    /// [`record` method] to record these values.
    ///
    /// [`Event`]: ::event::Event
    /// [visitor]: ::field::Visit
    /// [`record` method]: ::event::Event::record
    fn event(&self, event: &Event);

    /// Visits that a spanhas been entered.
    ///
    /// When entering a span, this method is called to notify the subscriber
    /// that the span has been entered. The subscriber is provided with the
    /// [span ID] of the entered span, and should update any internal state
    /// tracking the current span accordingly.
    ///
    /// [span ID]: ../span/struct.Id.html
    fn enter(&self, span: &span::Id);

    /// Visits that a span has been exited.
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
    /// [`drop_span`]: ::subscriber::Subscriber::drop_span
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
    /// [`drop_span`]: ::subscriber::Subscriber::drop_span
    fn drop_span(&self, id: span::Id) {
        let _ = id;
    }
}

/// Indicates a `Subscriber`'s interest in a particular callsite.
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
    /// callsite every time it creates a span. This will be the case until a
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
}
