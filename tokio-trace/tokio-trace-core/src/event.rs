//! Events represent single points in time during the execution of a program.
use std::fmt;
use {
    dispatcher,
    field::{self, Value},
    Metadata,
};

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

/// Constructs an `Event`.
#[derive(Debug)]
pub struct Builder<'a> {
    metadata: &'a Metadata<'a>,
}

impl<'a> Event<'a> {
    /// Returns a new `Builder` for constructing an `Event` with the specified
    /// `metadata`.
    pub fn builder(metadata: &'a Metadata<'a>) -> Builder<'a> {
        Builder::new(metadata)
    }

    /// Returns all the fields on this `Event` with the specified [recorder].
    ///
    /// [recorder]: ::field::Record
    #[inline]
    pub fn record(&self, recorder: &mut field::Record) {
        debug_assert!(self.fields.is_complete());
        self.fields.record(recorder);
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

impl<'a> Builder<'a> {
    /// Returns a new event `Builder`.
    pub fn new(metadata: &'a Metadata<'a>) -> Self {
        Self {
            metadata,
        }
    }

    /// Records the constructed `Event` with a set of values.
    pub fn record(&self, fields: &'a field::ValueSet<'a>) {
        let event = Event {
            metadata: self.metadata,
            fields,
        };
        dispatcher::with(|current| {
            current.event(&event);
        });
    }
}
