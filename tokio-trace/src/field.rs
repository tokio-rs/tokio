//! Structured data associated with `Span`s and `Event`s.
pub use tokio_trace_core::field::*;

use Metadata;

/// Trait implemented to allow a type to be used as a field key.
///
/// **Note**: Although this is implemented for both the [`Field`] type *and* any
/// type that can be borrowed as an `&str`, only `Field` allows _O_(1) access.
/// Indexing a field with a string results in an iterative search that performs
/// string comparisons. Thus, if possible, once the key for a field is known, it
/// should be used whenever possible.
///
/// [`Field`]: ../struct.Field.html
pub trait AsField: ::sealed::Sealed {
    /// Attempts to convert `&self` into a `Field` with the specified `metadata`.
    ///
    /// If `metadata` defines this field, then the field is returned. Otherwise,
    /// this returns `None`.
    fn as_field(&self, metadata: &Metadata) -> Option<Field>;
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
        metadata.fields().field(&self)
    }
}

impl ::sealed::Sealed for Field {}
impl<'a> ::sealed::Sealed for &'a Field {}
impl ::sealed::Sealed for str {}
