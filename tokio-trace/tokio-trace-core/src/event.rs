//! Events represent single points in time during the execution of a program.
use {field, Metadata};

/// `Event`s represent single points in time where something occurred during the
/// execution of a program.
///
/// An `Event` can be compared to a log record in unstructured logging, but with
/// two key differences:
/// - `Event`s exist _within the context of a [`Span`]_. Unlike log lines, they
///   may be located within the trace tree, allowing visibility into the
///   _temporal_ context in which the event occurred, as well as the source
///   code location.
/// - Like spans, `Event`s have structured key-value data known as _fields_,
///   which may include textual message. In general, a majority of the data
///   associated with an event should be in the event's fields rather than in
///   the textual message, as the fields are more structed.
///
/// [`Span`]: ::span::Span
#[derive(Debug)]
pub struct Event<'a> {
    fields: &'a field::ValueSet<'a>,
    metadata: &'a Metadata<'a>,
}

impl<'a> Event<'a> {
    /// Constructs a new `Event` with the specified metadata and set of values,
    /// and observes it with the current subscriber.
    #[inline]
    pub fn observe(metadata: &'a Metadata<'a>, fields: &'a field::ValueSet) {
        let event = Event { metadata, fields };
        ::dispatcher::with(|current| {
            current.event(&event);
        });
    }

    /// Visits all the fields on this `Event` with the specified [visitor].
    ///
    /// [visitor]: ::field::Visit
    #[inline]
    pub fn record(&self, visitor: &mut field::Visit) {
        self.fields.record(visitor);
    }

    /// Returns a reference to the set of values on this `Event`.
    pub fn fields(&self) -> field::Iter {
        self.fields.field_set().iter()
    }

    /// Returns [metadata] describing this `Event`.
    ///
    /// [metadata]: ::metadata::Metadata
    pub fn metadata(&self) -> &Metadata {
        self.metadata
    }
}
