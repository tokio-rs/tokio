//! Span and `Event` key-value data.
//!
//! Spans and events  may be annotated with key-value data, referred to as known
//! as _fields_. These fields consist of a mapping from a key (corresponding to
//! a `&str` but represented internally as an array index) to a [`Value`].
//!
//! # `Value`s and `Subscriber`s
//!
//! `Subscriber`s consume `Value`s as fields attached to [span]s or [`Event`]s.
//! The set of field keys on a given span or is defined on its [`Metadata`].
//! When a span is created, it provides [`Attributes`] to the `Subscriber`'s
//! [`new_span`] method, containing any fields whose values were provided when
//! the span was created; and may call the `Subscriber`'s [`record`] method
//! with additional [`Record`]s if values are added for more of its fields.
//! Similarly, the [`Event`] type passed to the subscriber's [`event`] method
//! will contain any fields attached to each event.
//!
//! `tokio_trace` represents values as either one of a set of Rust primitives
//! (`i64`, `u64`, `bool`, and `&str`) or using a `fmt::Display` or `fmt::Debug`
//! implementation. The [`record`] trait method on the `Subscriber` trait
//! allow `Subscriber` implementations to provide type-specific behaviour for
//! consuming values of each type.
//!
//! Instances of the [`Visit`] trait are provided by `Subscriber`s to record the
//! values attached to spans and `Event`. This trait represents the behavior
//! used to record values of various types. For example, we might record
//! integers by incrementing counters for their field names, rather than printing
//! them.
//!
//! [`Value`]: trait.Value.html
//! [span]: ../span/
//! [`Event`]: ../event/struct.Event.html
//! [`Metadata`]: ../metadata/struct.Metadata.html
//! [`Attributes`]:  ../span/struct.Attributes.html
//! [`Record`]: ../span/struct.Record.html
//! [`new_span`]: ../subscriber/trait.Subscriber.html#method.new_span
//! [`record`]: ../subscriber/trait.Subscriber.html#method.record
//! [`event`]:  ../subscriber/trait.Subscriber.html#method.record
//! [`Visit`]: trait.Visit.html
use callsite;
use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
};

use self::private::ValidLen;

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
    values: &'a [(&'a Field, Option<&'a (Value + 'a)>)],
    fields: &'a FieldSet,
}

/// An iterator over a set of fields.
#[derive(Debug)]
pub struct Iter {
    idxs: Range<usize>,
    fields: FieldSet,
}

/// Visits typed values.
///
/// An instance of `Visit` ("a visitor") represents the logic necessary to
/// record field values of various types. When an implementor of [`Value`] is
/// [recorded], it calls the appropriate method on the provided visitor to
/// indicate the type that value should be recorded as.
///
/// When a [`Subscriber`] implementation [records an `Event`] or a
/// [set of `Value`s added to a `Span`], it can pass an `&mut Visit` to the
/// `record` method on the provided [`ValueSet`] or [`Event`]. This visitor
/// will then be used to record all the field-value pairs present on that
/// `Event` or `ValueSet`.
///
/// # Examples
///
/// A simple visitor that writes to a string might be implemented like so:
/// ```
/// # extern crate tokio_trace_core as tokio_trace;
/// use std::fmt::{self, Write};
/// use tokio_trace::field::{Value, Visit, Field};
/// # fn main() {
/// pub struct StringVisitor<'a> {
///     string: &'a mut String,
/// }
///
/// impl<'a> Visit for StringVisitor<'a> {
///     fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
///         write!(self.string, "{} = {:?}; ", field.name(), value).unwrap();
///     }
/// }
/// # }
/// ```
/// This visitor will format each recorded value using `fmt::Debug`, and
/// append the field name and formatted value to the provided string,
/// regardless of the type of the recorded value. When all the values have
/// been recorded, the `StringVisitor` may be dropped, allowing the string
/// to be printed or stored in some other data structure.
///
/// The `Visit` trait provides default implementations for `record_i64`,
/// `record_u64`, `record_bool`, and `record_str` which simply forward the
/// recorded value to `record_debug`. Thus, `record_debug` is the only method
/// which a `Visit` implementation *must* implement. However, visitors may
/// override the default implementations of these functions in order to
/// implement type-specific behavior.
///
/// Additionally, when a visitor recieves a value of a type it does not care
/// about, it is free to ignore those values completely. For example, a
/// visitor which only records numeric data might look like this:
///
/// ```
/// # extern crate tokio_trace_core as tokio_trace;
/// # use std::fmt::{self, Write};
/// # use tokio_trace::field::{Value, Visit, Field};
/// # fn main() {
/// pub struct SumVisitor {
///     sum: i64,
/// }
///
/// impl Visit for SumVisitor {
///     fn record_i64(&mut self, _field: &Field, value: i64) {
///        self.sum += value;
///     }
///
///     fn record_u64(&mut self, _field: &Field, value: u64) {
///         self.sum += value as i64;
///     }
///
///     fn record_debug(&mut self, _field: &Field, _value: &fmt::Debug) {
///         // Do nothing
///     }
/// }
/// # }
/// ```
///
/// This visitor (which is probably not particularly useful) keeps a running
/// sum of all the numeric values it records, and ignores all other values. A
/// more practical example of recording typed values is presented in
/// `examples/counters.rs`, which demonstrates a very simple metrics system
/// implemented using `tokio-trace`.
///
/// [`Value`]: trait.Value.html
/// [recorded]: trait.Value.html#method.record
/// [`Subscriber`]: ../subscriber/trait.Subscriber.html
/// [records an `Event`]: ../subscriber/trait.Subscriber.html#method.event
/// [set of `Value`s added to a `Span`]: ../subscriber/trait.Subscriber.html#method.record
/// [`Event`]: ../event/struct.Event.html
/// [`ValueSet`]: struct.ValueSet.html
pub trait Visit {
    /// Visit a signed 64-bit integer value.
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_debug(field, &value)
    }

    /// Visit an umsigned 64-bit integer value.
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_debug(field, &value)
    }

    /// Visit a boolean value.
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_debug(field, &value)
    }

    /// Visit a string value.
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_debug(field, &value)
    }

    /// Visit a value implementing `fmt::Debug`.
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug);
}

/// A field value of an erased type.
///
/// Implementors of `Value` may call the appropriate typed recording methods on
/// the [visitor] passed to their `record` method in order to indicate how
/// their data should be recorded.
///
/// [visitor]: trait.Visit.html
pub trait Value: ::sealed::Sealed {
    /// Visits this value with the given `Visitor`.
    fn record(&self, key: &Field, visitor: &mut Visit);
}

/// A `Value` which serializes as a string using `fmt::Display`.
#[derive(Clone)]
pub struct DisplayValue<T: fmt::Display>(T);

/// A `Value` which serializes as a string using `fmt::Debug`.
#[derive(Clone)]
pub struct DebugValue<T: fmt::Debug>(T);

/// Wraps a type implementing `fmt::Display` as a `Value` that can be
/// recorded using its `Display` implementation.
pub fn display<T>(t: T) -> DisplayValue<T>
where
    T: fmt::Display,
{
    DisplayValue(t)
}

/// Wraps a type implementing `fmt::Debug` as a `Value` that can be
/// recorded using its `Debug` implementation.
pub fn debug<T>(t: T) -> DebugValue<T>
where
    T: fmt::Debug,
{
    DebugValue(t)
}

// ===== impl Visit =====

impl<'a, 'b> Visit for fmt::DebugStruct<'a, 'b> {
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        self.field(field.name(), value);
    }
}

impl<'a, 'b> Visit for fmt::DebugMap<'a, 'b> {
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        self.entry(&format_args!("{}", field), value);
    }
}

impl<F> Visit for F
where
    F: FnMut(&Field, &fmt::Debug),
{
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        (self)(field, value)
    }
}

// ===== impl Value =====

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
                    visitor: &mut $crate::field::Visit,
                ) {
                    visitor.$record(key, *self)
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
                    visitor: &mut $crate::field::Visit,
                ) {
                    visitor.$record(key, *self as $as_ty)
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
    fn record(&self, key: &Field, visitor: &mut Visit) {
        visitor.record_str(key, &self)
    }
}

impl<'a, T: ?Sized> ::sealed::Sealed for &'a T where T: Value + ::sealed::Sealed + 'a {}

impl<'a, T: ?Sized> Value for &'a T
where
    T: Value + 'a,
{
    fn record(&self, key: &Field, visitor: &mut Visit) {
        (*self).record(key, visitor)
    }
}

impl<'a> ::sealed::Sealed for fmt::Arguments<'a> {}

impl<'a> Value for fmt::Arguments<'a> {
    fn record(&self, key: &Field, visitor: &mut Visit) {
        visitor.record_debug(key, self)
    }
}

// ===== impl DisplayValue =====

impl<T: fmt::Display> ::sealed::Sealed for DisplayValue<T> {}

impl<T> Value for DisplayValue<T>
where
    T: fmt::Display,
{
    fn record(&self, key: &Field, visitor: &mut Visit) {
        visitor.record_debug(key, &format_args!("{}", self.0))
    }
}

impl<T: fmt::Display> fmt::Debug for DisplayValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ===== impl DebugValue =====

impl<T: fmt::Debug> ::sealed::Sealed for DebugValue<T> {}

impl<T: fmt::Debug> Value for DebugValue<T>
where
    T: fmt::Debug,
{
    fn record(&self, key: &Field, visitor: &mut Visit) {
        visitor.record_debug(key, &self.0)
    }
}

impl<T: fmt::Debug> fmt::Debug for DebugValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

// ===== impl Field =====

impl Field {
    /// Returns an [`Identifier`] that uniquely identifies the [`Callsite`]
    /// which defines this field.
    ///
    /// [`Identifier`]: ../callsite/struct.Identifier.html
    /// [`Callsite`]: ../callsite/trait.Callsite.html
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Returns a string representing the name of the field.
    pub fn name(&self) -> &'static str {
        self.fields.names[self.i]
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
    /// Returns an [`Identifier`] that uniquely identifies the [`Callsite`]
    /// which defines this set of fields..
    ///
    /// [`Identifier`]: ../callsite/struct.Identifier.html
    /// [`Callsite`]: ../callsite/trait.Callsite.html
    pub(crate) fn callsite(&self) -> callsite::Identifier {
        callsite::Identifier(self.callsite.0)
    }

    /// Returns the [`Field`] named `name`, or `None` if no such field exists.
    ///
    /// [`Field`]: ../struct.Field.html
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

    /// Returns a new `ValueSet` with entries for this `FieldSet`'s values.
    ///
    /// Note that a `ValueSet` may not be constructed with arrays of over 32
    /// elements.
    #[doc(hidden)]
    pub fn value_set<'v, V>(&'v self, values: &'v V) -> ValueSet<'v>
    where
        V: ValidLen<'v>,
    {
        ValueSet {
            fields: self,
            values: &values.borrow()[..],
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

impl fmt::Display for FieldSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_set()
            .entries(self.names.iter().map(|n| display(n)))
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
    /// Returns an [`Identifier`] that uniquely identifies the [`Callsite`]
    /// defining the fields this `ValueSet` refers to.
    ///
    /// [`Identifier`]: ../callsite/struct.Identifier.html
    /// [`Callsite`]: ../callsite/trait.Callsite.html
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Visits all the fields in this `ValueSet` with the provided [visitor].
    ///
    /// [visitor]: ../trait.Visit.html
    pub(crate) fn record(&self, visitor: &mut Visit) {
        let my_callsite = self.callsite();
        for (field, value) in self.values {
            if field.callsite() != my_callsite {
                continue;
            }
            if let Some(value) = value {
                value.record(field, visitor);
            }
        }
    }

    /// Returns `true` if this `ValueSet` contains a value for the given `Field`.
    pub(crate) fn contains(&self, field: &Field) -> bool {
        field.callsite() == self.callsite()
            && self
                .values
                .iter()
                .any(|(key, val)| *key == field && val.is_some())
    }

    /// Returns true if this `ValueSet` contains _no_ values.
    pub(crate) fn is_empty(&self) -> bool {
        let my_callsite = self.callsite();
        self.values
            .iter()
            .all(|(key, val)| val.is_none() || key.callsite() != my_callsite)
    }

    pub(crate) fn field_set(&self) -> &FieldSet {
        self.fields
    }
}

impl<'a> fmt::Debug for ValueSet<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.values
            .iter()
            .fold(&mut f.debug_struct("ValueSet"), |dbg, (key, v)| {
                if let Some(val) = v {
                    val.record(key, dbg);
                }
                dbg
            })
            .field("callsite", &self.callsite())
            .finish()
    }
}

impl<'a> fmt::Display for ValueSet<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.values
            .iter()
            .fold(&mut f.debug_map(), |dbg, (key, v)| {
                if let Some(val) = v {
                    val.record(key, dbg);
                }
                dbg
            })
            .finish()
    }
}

// ===== impl ValidLen =====

mod private {
    use super::*;

    /// Marker trait implemented by arrays which are of valid length to
    /// construct a `ValueSet`.
    ///
    /// `ValueSet`s may only be constructed from arrays containing 32 or fewer
    /// elements, to ensure the array is small enough to always be allocated on the
    /// stack. This trait is only implemented by arrays of an appropriate length,
    /// ensuring that the correct size arrays are used at compile-time.
    pub trait ValidLen<'a>: Borrow<[(&'a Field, Option<&'a (Value + 'a)>)]> {}
}

macro_rules! impl_valid_len {
    ( $( $len:tt ),+ ) => {
        $(
            impl<'a> private::ValidLen<'a> for
                [(&'a Field, Option<&'a (Value + 'a)>); $len] {}
        )+
    }
}

impl_valid_len! {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32
}

#[cfg(test)]
mod test {
    use super::*;
    use {Level, Metadata};

    struct TestCallsite1;
    static TEST_CALLSITE_1: TestCallsite1 = TestCallsite1;
    static TEST_META_1: Metadata<'static> = metadata! {
        name: "field_test1",
        target: module_path!(),
        level: Level::INFO,
        fields: &["foo", "bar", "baz"],
        callsite: &TEST_CALLSITE_1,
    };

    impl ::callsite::Callsite for TestCallsite1 {
        fn add_interest(&self, _: ::subscriber::Interest) {}
        fn clear_interest(&self) {}

        fn metadata(&self) -> &Metadata {
            &TEST_META_1
        }
    }

    struct TestCallsite2;
    static TEST_CALLSITE_2: TestCallsite2 = TestCallsite2;
    static TEST_META_2: Metadata<'static> = metadata! {
        name: "field_test2",
        target: module_path!(),
        level: Level::INFO,
        fields: &["foo", "bar", "baz"],
        callsite: &TEST_CALLSITE_2,
    };

    impl ::callsite::Callsite for TestCallsite2 {
        fn add_interest(&self, _: ::subscriber::Interest) {}
        fn clear_interest(&self) {}

        fn metadata(&self) -> &Metadata {
            &TEST_META_2
        }
    }

    #[test]
    fn value_set_with_no_values_is_empty() {
        let fields = TEST_META_1.fields();
        let values = &[
            (&fields.field("foo").unwrap(), None),
            (&fields.field("bar").unwrap(), None),
            (&fields.field("baz").unwrap(), None),
        ];
        let valueset = fields.value_set(values);
        assert!(valueset.is_empty());
    }

    #[test]
    fn empty_value_set_is_empty() {
        let fields = TEST_META_1.fields();
        let valueset = fields.value_set(&[]);
        assert!(valueset.is_empty());
    }

    #[test]
    fn value_sets_with_fields_from_other_callsites_are_empty() {
        let fields = TEST_META_1.fields();
        let values = &[
            (&fields.field("foo").unwrap(), Some(&1 as &Value)),
            (&fields.field("bar").unwrap(), Some(&2 as &Value)),
            (&fields.field("baz").unwrap(), Some(&3 as &Value)),
        ];
        let valueset = TEST_META_2.fields().value_set(values);
        assert!(valueset.is_empty())
    }

    #[test]
    fn sparse_value_sets_are_not_empty() {
        let fields = TEST_META_1.fields();
        let values = &[
            (&fields.field("foo").unwrap(), None),
            (&fields.field("bar").unwrap(), Some(&57 as &Value)),
            (&fields.field("baz").unwrap(), None),
        ];
        let valueset = fields.value_set(values);
        assert!(!valueset.is_empty());
    }

    #[test]
    fn fields_from_other_callsets_are_skipped() {
        let fields = TEST_META_1.fields();
        let values = &[
            (&fields.field("foo").unwrap(), None),
            (
                &TEST_META_2.fields().field("bar").unwrap(),
                Some(&57 as &Value),
            ),
            (&fields.field("baz").unwrap(), None),
        ];

        struct MyVisitor;
        impl Visit for MyVisitor {
            fn record_debug(&mut self, field: &Field, _: &::std::fmt::Debug) {
                assert_eq!(field.callsite(), TEST_META_1.callsite())
            }
        }
        let valueset = fields.value_set(values);
        valueset.record(&mut MyVisitor);
    }

    #[test]
    fn record_debug_fn() {
        let fields = TEST_META_1.fields();
        let values = &[
            (&fields.field("foo").unwrap(), Some(&1 as &Value)),
            (&fields.field("bar").unwrap(), Some(&2 as &Value)),
            (&fields.field("baz").unwrap(), Some(&3 as &Value)),
        ];
        let valueset = fields.value_set(values);
        let mut result = String::new();
        valueset.record(&mut |_: &Field, value: &fmt::Debug| {
            use std::fmt::Write;
            write!(&mut result, "{:?}", value).unwrap();
        });
        assert_eq!(result, "123".to_owned());
    }
}
