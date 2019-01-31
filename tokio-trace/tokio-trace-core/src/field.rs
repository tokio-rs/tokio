//! `Span` and `Event` key-value data.
//!
//! Spans and events  may be annotated with key-value data, referred to as known
//! as _fields_. These fields consist of a mapping from a key (corresponding to
//! a `&str` but represented internally as an array index) to a `Value`.
//!
//! # `Value`s and `Subscriber`s
//!
//! `Subscriber`s consume `Value`s as fields attached to `Span`s or `Event`s.
//! The set of field keys on a given `Span` or is defined on its `Metadata`.
//! When a `Span` is created, it provides a `ValueSet` to the `Subscriber`'s
//! [`new_span`] method, containing any fields whose values were provided when
//! the span was created; and may call the `Subscriber`'s [`record`] method
//! with additional `ValueSet`s if values are added for more of its fields.
//! Similarly, the [`Event`] type passed to the subscriber's [`event`] method
//! will contain any fields attached to each event.
//!
//! `tokio_trace` represents values as either one of a set of Rust primitives
//! (`i64`, `u64`, `bool`, and `&str`) or using a `fmt::Display` or `fmt::Debug`
//! implementation. The `record_` trait functions on the `Subscriber` trait
//! allow `Subscriber` implementations to provide type-specific behaviour for
//! consuming values of each type.
//!
//! Instances of the `Record` trait are provided by `Subscriber`s to record the
//! values attached to `Span`s and `Event`. This trait represents the behavior
//! used to record values of various types. For example, we might record
//! integers by incrementing counters for their field names, rather than printing
//! them.
//!
//! [`new_span`]: ::subscriber::Subscriber::new_span
//! [`record`]: ::subscriber::Subscriber::record
//! [`Event`]: ::event::Event
//! [`event`]: ::subscriber::Subscriber::event
use callsite;
use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
};

/// An opaque key allowing _O_(1) access to a field in a `Span`'s key-value
/// data.
///
/// As keys are defined by the _metadata_ of a span, rather than by an
/// individual instance of a span, a key may be used to access the same field
/// across all instances of a given span with the same metadata. Thus, when a
/// subscriber observes a new span, it need only access a field by name _once_,
/// and use the key for that name for all other accesses.
#[derive(Debug)]
pub struct Field {
    i: usize,
    fields: FieldSet,
}

/// Describes the fields present on a span.
// TODO: When `const fn` is stable, make this type's fields private.
pub struct FieldSet {
    /// The names of each field on the described span.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must be able
    /// to be constructed statically by macros. However, when `const fn`s are
    /// available on stable Rust, this will no longer be necessary. Thus, these
    /// fields are *not* considered stable public API, and they may change
    /// warning. Do not rely on any fields on `FieldSet`!
    #[doc(hidden)]
    pub names: &'static [&'static str],
    /// The callsite where the described span originates.
    ///
    /// **Warning**: The fields on this type are currently `pub` because it must be able
    /// to be constructed statically by macros. However, when `const fn`s are
    /// available on stable Rust, this will no longer be necessary. Thus, these
    /// fields are *not* considered stable public API, and they may change
    /// warning. Do not rely on any fields on `FieldSet`!
    #[doc(hidden)]
    pub callsite: callsite::Identifier,
}

/// A set of fields and values for a span.
pub struct ValueSet<'a> {
    values: [Option<&'a Value>; 32],
    fields: &'a FieldSet,
    is_complete: bool,
}

/// An iterator over a set of fields.
#[derive(Debug)]
pub struct Iter {
    idxs: Range<usize>,
    fields: FieldSet,
}

/// Records typed values.
///
/// An instance of `Record` ("a recorder") represents the logic necessary to
/// record field values of various types. When an implementor of [`Value`] is
/// [recorded], it calls the appropriate method on the provided recorder to
/// indicate the type that value should be recorded as.
///
/// When a [`Subscriber`] implementation [records an `Event`] or a
/// [set of `Value`s added to a `Span`], it can pass an `&mut Record` to the
/// `record` method on the provided [`ValueSet`] or [`Event`]. This recorder
/// will then be used to record all the field-value pairs present on that
/// `Event` or `ValueSet`.
///
/// # Examples
///
/// A simple recorder that writes to a string might be implemented like so:
/// ```
/// # extern crate tokio_trace_core as tokio_trace;
/// use std::fmt::{self, Write};
/// use tokio_trace::field::{Value, Record, Field};
/// # fn main() {
/// pub struct StringRecorder<'a> {
///     string: &'a mut String,
/// }
///
/// impl<'a> Record for StringRecorder<'a> {
///     fn record_i64(&mut self, field: &Field, value: i64) {
///         self.record_debug(field, &value)
///     }
///
///     fn record_u64(&mut self, field: &Field, value: u64) {
///         self.record_debug(field, &value)
///     }
///
///     fn record_bool(&mut self, field: &Field, value: bool) {
///         self.record_debug(field, &value)
///     }
///
///    fn record_str(&mut self, field: &Field, value: &str) {
///         self.record_debug(field, &value)
///     }
///
///     fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
///         write!(self.string, "{} = {:?}; ", field.name(), value).unwrap();
///     }
/// }
/// # }
/// ```
/// This recorder will format each recorded value using `fmt::Debug`, and
/// append the field name and formatted value to the provided string,
/// regardless of the type of the recorded value. When all the values have
/// been recorded, the `StringRecorder` may be dropped, allowing the string
/// to be printed or stored in some other data structure.
///
/// While the `StringRecorder` implements the `record_i64`, `record_u64`,
/// `record_bool`, and `record_str` functions by forwarding to `record_debug`,
/// other recorders may implement type-specific behavior in those functions.
/// Additionally, when a recorder recieves a value of a type it does not care
/// about, it is free to ignore those values completely – for example, a
/// recorder which only records numeric data might look like this:
///
/// ```
/// # extern crate tokio_trace_core as tokio_trace;
/// # use std::fmt::{self, Write};
/// # use tokio_trace::field::{Value, Record, Field};
/// # fn main() {
/// pub struct SumRecorder {
///     sum: i64,
/// }
///
/// impl Record for SumRecorder {
///     fn record_i64(&mut self, _field: &Field, value: i64) {
///        self.sum += value;
///     }
///
///     fn record_u64(&mut self, _field: &Field, value: u64) {
///         self.sum += value as i64;
///     }
///
///     fn record_bool(&mut self, _field: &Field, _value: bool) {
///         // Do nothing
///     }
///
///    fn record_str(&mut self, _field: &Field, _value: &str) {
///         // Do nothing
///     }
///
///     fn record_debug(&mut self, _field: &Field, _value: &fmt::Debug) {
///         // Do nothing
///     }
/// }
/// # }
/// ```
/// This recorder (which is probably not particularly useful) keeps a running
/// sum of all the numeric values it records, and ignores all other values. A
/// more practical example of recording typed values is presented in
/// `examples/counters.rs`, which demonstrates a very simple metrics system
/// implemented using `tokio-trace`.
///
/// [`Value`]: ::field::Value
/// [recorded]: ::field::Value::record
/// [`Subscriber`]: ::subscriber::Subscriber
/// [records an `Event`]: ::subscriber::Subscriber::event
/// [set of `Value`s added to a `Span`]: ::subscriber::Subscriber::record
/// [`Event`]: ::event::Event
/// [`ValueSet`]: ::field::ValueSet
pub trait Record {
    /// Record a signed 64-bit integer value.
    fn record_i64(&mut self, field: &Field, value: i64);

    /// Record an umsigned 64-bit integer value.
    fn record_u64(&mut self, field: &Field, value: u64);

    /// Record a boolean value.
    fn record_bool(&mut self, field: &Field, value: bool);

    /// Record a string value.
    fn record_str(&mut self, field: &Field, value: &str);

    /// Record a value implementing `fmt::Debug`.
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug);
}

/// A field value of an erased type.
///
/// Implementors of `Value` may call the appropriate typed recording methods on
/// the [recorder] passed to their `record` method in order to indicate how
/// their data should be recorded.
///
/// [recorder]: ::field::Record
pub trait Value: ::sealed::Sealed {
    /// Records this value with the given `Recorder`.
    fn record(&self, key: &Field, recorder: &mut Record);
}

/// A `Value` which serializes as a string using `fmt::Display`.
#[derive(Debug, Clone)]
pub struct DisplayValue<T: fmt::Display>(T);

/// A `Value` which serializes as a string using `fmt::Debug`.
#[derive(Debug, Clone)]
pub struct DebugValue<T: fmt::Debug>(T);

/// Wraps a type implementing `fmt::Display` as a `Value` that can be
/// recorded using its `Display` implementation.
pub fn display<T>(t: T) -> DisplayValue<T>
where
    T: fmt::Display,
{
    DisplayValue(t)
}

// ===== impl Value =====

/// Wraps a type implementing `fmt::Debug` as a `Value` that can be
/// recorded using its `Debug` implementation.
pub fn debug<T>(t: T) -> DebugValue<T>
where
    T: fmt::Debug,
{
    DebugValue(t)
}

macro_rules! impl_values {
    ( $( $record:ident( $( $whatever:tt)+ ) ),+ ) => {
        $(
            impl_value!{ $record( $( $whatever )+ ) }
        )+
    }
}
macro_rules! impl_value {
    ( $record:ident( $( $value_ty:ty ),+ ) ) => {
        $(
            impl $crate::sealed::Sealed for $value_ty {}
            impl $crate::field::Value for $value_ty {
                fn record(
                    &self,
                    key: &$crate::field::Field,
                    recorder: &mut $crate::field::Record,
                ) {
                    recorder.$record(key, *self)
                }
            }
        )+
    };
    ( $record:ident( $( $value_ty:ty ),+ as $as_ty:ty) ) => {
        $(
            impl $crate::sealed::Sealed for $value_ty {}
            impl Value for $value_ty {
                fn record(
                    &self,
                    key: &$crate::field::Field,
                    recorder: &mut $crate::field::Record,
                ) {
                    recorder.$record(key, *self as $as_ty)
                }
            }
        )+
    };
}

// ===== impl Value =====

impl_values! {
    record_u64(u64),
    record_u64(usize, u32, u16 as u64),
    record_i64(i64),
    record_i64(isize, i32, i16, i8 as i64),
    record_bool(bool)
}

impl ::sealed::Sealed for str {}

impl Value for str {
    fn record(&self, key: &Field, recorder: &mut Record) {
        recorder.record_str(key, &self)
    }
}

impl<'a, T: ?Sized> ::sealed::Sealed for &'a T where T: Value + ::sealed::Sealed + 'a {}

impl<'a, T: ?Sized> Value for &'a T
where
    T: Value + 'a,
{
    fn record(&self, key: &Field, recorder: &mut Record) {
        (*self).record(key, recorder)
    }
}

// ===== impl DisplayValue =====

impl<T: fmt::Display> ::sealed::Sealed for DisplayValue<T> {}

impl<T> Value for DisplayValue<T>
where
    T: fmt::Display,
{
    fn record(&self, key: &Field, recorder: &mut Record) {
        recorder.record_debug(key, &format_args!("{}", self.0))
    }
}

// ===== impl DebugValue =====
impl<T: fmt::Debug> ::sealed::Sealed for DebugValue<T> {}

impl<T: fmt::Debug> Value for DebugValue<T>
where
    T: fmt::Debug,
{
    fn record(&self, key: &Field, recorder: &mut Record) {
        recorder.record_debug(key, &self.0)
    }
}

// ===== impl Field =====

impl Field {
    /// Returns an [`Identifier`](::metadata::Identifier) that uniquely
    /// identifies the callsite that defines the field this key refers to.
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Returns a string representing the name of the field.
    pub fn name(&self) -> &'static str {
        self.fields.names[self.i]
    }

    /// Constructs a new `ValueSet` containing this field and the given value.
    pub fn with_value<'a>(&'a self, value: &'a Value) -> ValueSet<'a> {
        let mut values = [None; 32];
        values[self.i] = Some(value);
        ValueSet::new(&self.fields, values)
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(self.name())
    }
}

impl AsRef<str> for Field {
    fn as_ref(&self) -> &str {
        self.name()
    }
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        self.callsite() == other.callsite() && self.i == other.i
    }
}

impl Eq for Field {}

impl Hash for Field {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.callsite().hash(state);
        self.i.hash(state);
    }
}

impl Clone for Field {
    fn clone(&self) -> Self {
        Field {
            i: self.i,
            fields: FieldSet {
                names: self.fields.names,
                callsite: self.fields.callsite(),
            },
        }
    }
}

// ===== impl FieldSet =====

impl FieldSet {
    pub(crate) fn callsite(&self) -> callsite::Identifier {
        callsite::Identifier(self.callsite.0)
    }

    /// Returns the [`Field`](::field::Field) named `name`, or `None` if no such
    /// field exists.
    pub fn field<Q: ?Sized>(&self, name: &Q) -> Option<Field>
    where
        Q: Borrow<str>,
    {
        let name = &name.borrow();
        self.names.iter().position(|f| f == name).map(|i| Field {
            i,
            fields: FieldSet {
                names: self.names,
                callsite: self.callsite(),
            },
        })
    }

    /// Returns `true` if `self` contains the given `field`.
    ///
    /// **Note**: If `field` shares a name with a field in this `FieldSet`, but
    /// was created by a `FieldSet` with a different callsite, this `FieldSet`
    /// does _not_ contain it. This is so that if two separate span callsites
    /// define a field named "foo", the `Field` corresponding to "foo" for each
    /// of those callsites are not equivalent.
    pub fn contains(&self, field: &Field) -> bool {
        field.callsite() == self.callsite() && field.i <= self.len()
    }

    /// Returns an iterator over the `Field`s in this `FieldSet`.
    pub fn iter(&self) -> Iter {
        let idxs = 0..self.len();
        Iter {
            idxs,
            fields: FieldSet {
                names: self.names,
                callsite: self.callsite(),
            },
        }
    }

    /// Returns the number of fields in this `FieldSet`.
    #[inline]
    pub fn len(&self) -> usize {
        self.names.len()
    }
}

impl<'a> IntoIterator for &'a FieldSet {
    type IntoIter = Iter;
    type Item = Field;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Debug for FieldSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FieldSet")
            .field("names", &self.names)
            .field("callsite", &self.callsite)
            .finish()
    }
}

// ===== impl Iter =====

impl Iterator for Iter {
    type Item = Field;
    fn next(&mut self) -> Option<Field> {
        let i = self.idxs.next()?;
        Some(Field {
            i,
            fields: FieldSet {
                names: self.fields.names,
                callsite: self.fields.callsite(),
            },
        })
    }
}

// ===== impl ValueSet =====

impl<'a> ValueSet<'a> {
    /// Returns a new `ValueSet`.
    pub fn new(fields: &'a FieldSet, values: [Option<&'a Value>; 32]) -> Self {
        let is_complete = values.iter().all(Option::is_some);
        ValueSet {
            values,
            fields,
            is_complete,
        }
    }

    /// Returns an [`Identifier`](::metadata::Identifier) that uniquely
    /// identifies the callsite that defines the fields this `ValueSet` refers to.
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Records all the fields in this `ValueSet` with the provided [recorder].
    ///
    /// [recorder]: ::field::Record
    pub fn record(&self, recorder: &mut Record) {
        if self.fields.callsite() != self.callsite() {
            return;
        }
        let fields = self.fields.iter().filter_map(|field| {
            let value = self.values[field.i]?;
            Some((field, value))
        });
        for (ref field, value) in fields {
            value.record(field, recorder);
        }
    }

    /// Returns `true` if this `ValueSet` contains a value for the given `Field`.
    pub fn contains(&self, field: &Field) -> bool {
        field.callsite() == self.callsite() && self.contains_inner(field)
    }

    fn contains_inner(&self, field: &Field) -> bool {
        self.values[field.i].is_some()
    }

    /// Returns true if this `ValueSet` contains _all_ the fields defined on the
    /// span or event it corresponds to.
    pub fn is_complete(&self) -> bool {
        if self.fields.callsite() != self.callsite() {
            return false;
        }
        for ref field in self.fields.iter() {
            if !self.contains_inner(field) {
                return false;
            }
        }
        true
    }

    /// Returns true if this `ValueSet` contains _no_ values.
    pub fn is_empty(&self) -> bool {
        if self.fields.callsite() != self.callsite() {
            return true;
        }
        for ref field in self.fields.iter() {
            if self.contains_inner(field) {
                return false;
            }
        }
        true
    }

    pub(crate) fn field_set(&self) -> &FieldSet {
        self.fields
    }
}

impl<'a> fmt::Debug for ValueSet<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ValueSet")
            .field("fields", &self.fields)
            .field("values", &format_args!("{}", "[...]"))
            .finish()
    }
}
