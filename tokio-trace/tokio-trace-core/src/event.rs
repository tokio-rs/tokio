//! Events represent single points in time during the execution of a program.
use parent::Parent;
use span::Id;
use {field, Metadata};

/// `Event`s represent single points in time where something occurred during the
/// execution of a program.
///
/// An `Event` can be compared to a log record in unstructured logging, but with
/// two key differences:
/// - `Event`s exist _within the context of a [span]_. Unlike log lines, they
///   may be located within the trace tree, allowing visibility into the
///   _temporal_ context in which the event occurred, as well as the source
///   code location.
/// - Like spans, `Event`s have structured key-value data known as _[fields]_,
///   which may include textual message. In general, a majority of the data
///   associated with an event should be in the event's fields rather than in
///   the textual message, as the fields are more structed.
///
/// [span]: ../span
/// [fields]: ../field
#[derive(Debug)]
pub struct Event<'a> {
    fields: &'a field::ValueSet<'a>,
    metadata: &'a Metadata<'a>,
    parent: Parent,
}

impl<'a> Event<'a> {
    /// Constructs a new `Event` with the specified metadata and set of values,
    /// and observes it with the current subscriber.
    #[inline]
    pub fn dispatch(metadata: &'a Metadata<'a>, fields: &'a field::ValueSet) {
        let event = Event {
            metadata,
            fields,
            parent: Parent::Current,
        };
        ::dispatcher::get_default(|current| {
            current.event(&event);
        });
    }

    /// Constructs a new `Event` with the specified metadata and set of values,
    /// and observes it with the current subscriber and an explicit parent.
    #[inline]
    pub fn child_of(
        parent: impl Into<Option<Id>>,
        metadata: &'a Metadata<'a>,
        fields: &'a field::ValueSet,
    ) {
        let parent = match parent.into() {
            Some(p) => Parent::Explicit(p),
            None => Parent::Root,
        };

        let event = Event {
            metadata,
            fields,
            parent,
        };
        ::dispatcher::get_default(|current| {
            current.event(&event);
        });
    }

    /// Visits all the fields on this `Event` with the specified [visitor].
    ///
    /// [visitor]: ../field/trait.Visit.html
    #[inline]
    pub fn record(&self, visitor: &mut field::Visit) {
        self.fields.record(visitor);
    }

    /// Returns an iterator over the set of values on this `Event`.
    pub fn fields(&self) -> field::Iter {
        self.fields.field_set().iter()
    }

    /// Returns [metadata] describing this `Event`.
    ///
    /// [metadata]: ../metadata/struct.Metadata.html
    pub fn metadata(&self) -> &Metadata {
        self.metadata
    }

    /// Returns true if the new event shoold be a root.
    pub fn is_root(&self) -> bool {
        match self.parent {
            Parent::Root => true,
            _ => false,
        }
    }

    /// Returns true if the new event's parent should be determined based on the
    /// current context.
    ///
    /// If this is true and the current thread is currently inside a span, then
    /// that span should be the new events's parent. Otherwise, if the current
    /// thread is _not_ inside a span, then the new event will be the root of its
    /// own trace tree.
    pub fn is_contextual(&self) -> bool {
        match self.parent {
            Parent::Current => true,
            _ => false,
        }
    }

    /// Returns the new event's explicitly-specified parent, if there is one.
    ///
    /// Otherwise (if the new event is a root or is a child of the current span),
    /// returns false.
    pub fn parent(&self) -> Option<&Id> {
        match self.parent {
            Parent::Explicit(ref p) => Some(p),
            _ => None,
        }
    }
}
