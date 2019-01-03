pub use tokio_trace_core::field::*;

use std::fmt;
use Metadata;

/// Trait implemented to allow a type to be used as a field key.
///
/// **Note**: Although this is implemented for both the [`Field`] type *and* any
/// type that can be borrowed as an `&str`, only `Field` allows _O_(1) access.
/// Indexing a field with a string results in an iterative search that performs
/// string comparisons. Thus, if possible, once the key for a field is known, it
/// should be used whenever possible.
pub trait AsField {
    /// Attempts to convert `&self` into a `Field` with the specified `metadata`.
    ///
    /// If `metadata` defines this field, then the field is returned. Otherwise,
    /// this returns `None`.
    fn as_field(&self, metadata: &Metadata) -> Option<Field>;
}

pub trait Record {
    /// Record a signed 64-bit integer value.
    fn record_i64<Q: ?Sized>(&mut self, field: &Q, value: i64)
    where
        Q: AsField;

    /// Record an umsigned 64-bit integer value.
    fn record_u64<Q: ?Sized>(&mut self, field: &Q, value: u64)
    where
        Q: AsField;

    /// Record a boolean value.
    fn record_bool<Q: ?Sized>(&mut self, field: &Q, value: bool)
    where
        Q: AsField;

    /// Record a string value.
    fn record_str<Q: ?Sized>(&mut self, field: &Q, value: &str)
    where
        Q: AsField;

    /// Record a value implementing `fmt::Debug`.
    fn record_debug<Q: ?Sized>(&mut self, field: &Q, value: &fmt::Debug)
    where
        Q: AsField;
}

/// A field value of an erased type.
///
/// Implementors of `Value` may call the appropriate typed recording methods on
/// the `Subscriber` passed to `record` in order to indicate how their data
/// should be recorded.
pub trait Value {
    /// Records this value with the given `Subscriber`.
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R)
    where
        Q: AsField,
        R: Record;
}

/// A `Value` which serializes as a string using `fmt::Display`.
#[derive(Clone)]
pub struct DisplayValue<T: fmt::Display>(T);

/// A `Value` which serializes as a string using `fmt::Debug`.
#[derive(Clone)]
pub struct DebugValue<T: fmt::Debug>(T);

/// Wraps a type implementing `fmt::Display` as a `Value` that can be
/// recorded using its `Display` implementation.
pub fn display<'a, T>(t: T) -> DisplayValue<T>
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
            impl $crate::field::Value for $value_ty {
                fn record<Q: ?Sized, R>(
                    &self,
                    key: &Q,
                    recorder: &mut R,
                )
                where
                    Q: $crate::field::AsField,
                    R: $crate::field::Record,
                {
                    recorder.$record(key, *self)
                }
            }
        )+
    };
    ( $record:ident( $( $value_ty:ty ),+ as $as_ty:ty) ) => {
        $(
            impl Value for $value_ty {
                fn record<Q: ?Sized, R>(
                    &self,
                    key: &Q,
                    recorder: &mut R,
                )
                where
                    Q: $crate::field::AsField,
                    R: $crate::field::Record,
                {
                    recorder.$record(key, *self as $as_ty)
                }
            }
        )+
    };
}

// ===== impl AsField =====

impl AsField for Field {
    #[inline]
    fn as_field(&self, metadata: &Metadata) -> Option<Field> {
        if self.callsite() == metadata.callsite() {
            Some(self.clone())
        } else {
            None
        }
    }
}

impl<'a> AsField for &'a Field {
    #[inline]
    fn as_field(&self, metadata: &Metadata) -> Option<Field> {
        if self.callsite() == metadata.callsite() {
            Some((*self).clone())
        } else {
            None
        }
    }
}

impl AsField for str {
    #[inline]
    fn as_field(&self, metadata: &Metadata) -> Option<Field> {
        metadata.fields().field_named(&self)
    }
}

// ===== impl Value =====

impl_values! {
    record_u64(u64),
    record_u64(usize, u32, u16 as u64),
    record_i64(i64),
    record_i64(isize, i32, i16, i8 as i64),
    record_bool(bool)
}

impl Value for str {
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R)
    where
        Q: AsField,
        R: Record,
    {
        recorder.record_str(key, &self)
    }
}

impl<'a, T: ?Sized> Value for &'a T
where
    T: Value + 'a,
{
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R)
    where
        Q: AsField,
        R: Record,
    {
        (*self).record(key, recorder)
    }
}

// ===== impl DisplayValue =====

impl<T> Value for DisplayValue<T>
where
    T: fmt::Display,
{
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R)
    where
        Q: AsField,
        R: Record,
    {
        recorder.record_debug(key, &format_args!("{}", self.0))
    }
}

// ===== impl DebugValue =====

impl<T: fmt::Debug> Value for DebugValue<T>
where
    T: fmt::Debug,
{
    fn record<Q: ?Sized, R>(&self, key: &Q, recorder: &mut R)
    where
        Q: AsField,
        R: Record,
    {
        recorder.record_debug(key, &self.0)
    }
}
