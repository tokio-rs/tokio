use super::SizeHint;

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Conversion from a `BufStream`.
///
/// By implementing `FromBufStream` for a type, you define how it will be
/// created from a buf stream. This is common for types which describe byte
/// storage of some kind.
///
/// `FromBufStream` is rarely called explicitly, and it is instead used through
/// `BufStream`'s `collect` method.
pub trait FromBufStream<T: Buf> {
    /// Type that is used to build `Self` while the `BufStream` is being
    /// consumed.
    type Builder;

    /// Create a new, empty, builder. The provided `hint` can be used to inform
    /// reserving capacity.
    fn builder(hint: &SizeHint) -> Self::Builder;

    /// Extend the builder with the `Buf`.
    ///
    /// This method is called whenever a new `Buf` value is obtained from the
    /// buf stream.
    fn extend(builder: &mut Self::Builder, buf: &mut T);

    /// Finalize the building of `Self`.
    ///
    /// Called once the buf stream is fully consumed.
    fn build(builder: Self::Builder) -> Self;
}

impl<T: Buf> FromBufStream<T> for Vec<u8> {
    type Builder = Vec<u8>;

    fn builder(hint: &SizeHint) -> Vec<u8> {
        Vec::with_capacity(hint.lower())
    }

    fn extend(builder: &mut Self, buf: &mut T) {
        builder.put(buf);
    }

    fn build(builder: Self) -> Self {
        builder
    }
}

impl<T: Buf> FromBufStream<T> for Bytes {
    type Builder = BytesMut;

    fn builder(hint: &SizeHint) -> BytesMut {
        BytesMut::with_capacity(hint.lower())
    }

    fn extend(builder: &mut Self::Builder, buf: &mut T) {
        builder.put(buf);
    }

    fn build(builder: Self::Builder) -> Self {
        builder.freeze()
    }
}
