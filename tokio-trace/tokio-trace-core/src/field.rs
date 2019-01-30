//! `Span` key-value data.
//!
//! Spans may be annotated with key-value data, referred to as known as
//! _fields_. These fields consist of a mapping from a key (corresponding to a
//! `&str` but represented internally as an array index) to a `Value`.
//!
//! # `Value`s and `Subscriber`s
//!
//! `Subscriber`s consume `Value`s as fields attached to `Span`s. The set of
//! field keys on a given `Span` or is defined on its `Metadata`. Once the span
//! has been created (i.e., the `new_id` or `new_span` methods on the
//! `Subscriber` have been called), field values may be added by calls to the
//! subscriber's `record_` methods.
//!
//! `tokio_trace` represents values as either one of a set of Rust primitives
//! (`i64`, `u64`, `bool`, and `&str`) or using a `fmt::Display` or `fmt::Debug`
//! implementation. The `record_` trait functions on the `Subscriber` trait
//! allow `Subscriber` implementations to provide type-specific behaviour for
//! consuming values of each type.
//!
//! The `Subscriber` trait provides default implementations of `record_u64`,
//! `record_i64`, `record_bool`, and `record_str` which call the `record_fmt`
//! function, so that only the `record_fmt` function must be implemented.
//! However, implementors of `Subscriber` that wish to consume these primitives
//! as their types may override the `record` methods for any types they care
//! about. For example, we might record integers by incrementing counters for
//! their field names, rather than printing them.
//!
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
    values: [Option<&'a dyn Value>; 32],
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
/// the `Subscriber` passed to `record` in order to indicate how their data
/// should be recorded.
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

impl<'a, T: ?Sized> ::sealed::Sealed for &'a T
where
    T: Value + ::sealed::Sealed + 'a,
{}

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
    pub fn new(
        fields: &'a FieldSet,
        values: [Option<&'a dyn Value>; 32],
    ) -> Self {
        let is_complete = values.iter().all(Option::is_some);
        ValueSet {
            values,
            fields,
            is_complete,
        }
    }

    /// Returns an [`Identifier`](::metadata::Identifier) that uniquely
    /// identifies the callsite that defines the fields this batch refers to.
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Records all the fields in this `ValueSet` with the provided `recorder`.
    pub fn record(&self, recorder: &mut Record) {
        if self.fields.callsite() != self.callsite() {
            return;
        }
        let fields = self.fields.iter()
            .filter_map(|field| {
                let value = self.values.get(field.i)?.as_ref()?;
                Some((field, value))
            });
        for (ref field, value) in fields {
            value.record(field, recorder);
        }
    }

    /// Returns the value for the given key, if one exists.
    pub fn get(&self, key: &Field) -> Option<&Value> {
        if self.callsite() != key.callsite() {
            return None;
        }
        self.values.get(key.i)?.as_ref().map(|&v| v)
    }

    /// Returns true if this `ValueSet` contains _all_ the fields defined on the
    /// span or event it corresponds to.
    pub fn is_complete(&self) -> bool {
        self.is_complete
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
            .field("is_complete", &self.is_complete)
            .finish()
    }
}
