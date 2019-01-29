//! Events represent single points in time during the execution of a program.
use ::{field::{self, Value}, Metadata};
use std::fmt;

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
    fields: field::ValueSet<'a>,
    metadata: &'a Metadata<'a>,
    message: Option<(field::Field, fmt::Arguments<'a>)>,
}

/// Constructs an `Event`.
#[derive(Debug)]
pub struct Builder<'a> {
    event: Event<'a>,
}

impl<'a> Event<'a> {
    /// Returns a new `Builder` for constructing an `Event` with the specified
    /// `metadata` and `fields`.
    pub fn builder(metadata: &'a Metadata<'a>, fields: field::ValueSet<'a>) -> Builder<'a> {
        Builder::new(metadata, fields)
    }

    /// Returns all the fields on this `Event` with the specified [recorder].
    ///
    /// [recorder]: ::field::Record
    pub fn record(&self, recorder: &mut field::Record) {
        if let Some((ref key, ref args)) = self.message {
            field::debug(args).record(key, recorder);
        }
        self.fields.record(recorder);
    }

    /// Returns an iterator over the fields in this event.
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
    pub fn new(metadata: &'a Metadata<'a>, fields: field::ValueSet<'a>) -> Self {
        Self {
            event: Event {
                fields,
                metadata,
                message: None,
            }
        }
    }

    /// Adds a message to the event.
    pub fn with_message(mut self, key: field::Field, message: fmt::Arguments<'a>) -> Self {
        if key.callsite() != self.event.metadata.callsite() {
            return self;
        }
        self.event.message = Some((key, message));
        self
    }

    /// Consumes the builder and returns the constructed `Event`.
    pub fn finish(self) -> Event<'a> {
        self.event
    }
}
