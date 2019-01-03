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

/// An iterator over a set of fields.
pub struct Iter {
    idxs: Range<usize>,
    fields: FieldSet,
}

// ===== impl Field =====

impl Field {
    /// Returns an [`Identifier`](::metadata::Identifier) that uniquely
    /// identifies the callsite that defines the field this key refers to.
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Returns a string representing the name of the field, or `None` if the
    /// field does not exist.
    pub fn name(&self) -> Option<&'static str> {
        self.fields.names.get(self.i).map(|&n| n)
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(self.name().unwrap_or("???"))
    }
}

impl AsRef<str> for Field {
    fn as_ref(&self) -> &str {
        self.name().unwrap_or("???")
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
    pub fn field_named<Q>(&self, name: &Q) -> Option<Field>
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
        field.callsite() == self.callsite() && field.i <= self.names.len()
    }

    /// Returns an iterator over the `Field`s in this `FieldSet`.
    pub fn iter(&self) -> Iter {
        let idxs = 0..self.names.len();
        Iter {
            idxs,
            fields: FieldSet {
                names: self.names,
                callsite: self.callsite(),
            },
        }
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
