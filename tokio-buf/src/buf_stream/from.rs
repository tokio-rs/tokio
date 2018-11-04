use super::SizeHint;

use bytes::{Buf, BufMut};

use std::usize;

/// Conversion from a `BufStream`.
///
/// By implementing `FromBufStream` for a type, you define how it will be
/// created from a buf stream. This is common for types which describe byte
/// storage of some kind.
///
/// `FromBufStream` is rarely called explicitly, and it is instead used through
/// `BufStream`'s `collect` method.
pub trait FromBufStream<T: Buf>: Sized {
    /// Type that is used to build `Self` while the `BufStream` is being
    /// consumed.
    type Builder;

    /// Error that might happen on conversion.
    type Error;

    /// Create a new, empty, builder. The provided `hint` can be used to inform
    /// reserving capacity.
    fn builder(hint: &SizeHint) -> Self::Builder;

    /// Extend the builder with the `Buf`.
    ///
    /// This method is called whenever a new `Buf` value is obtained from the
    /// buf stream.
    ///
    /// The provided size hint represents the state of the stream **after**
    /// `buf` has been yielded. The lower bound represents the minimum amount of
    /// data that will be provided after this call to `extend` returns.
    fn extend(builder: &mut Self::Builder, buf: &mut T, hint: &SizeHint)
        -> Result<(), Self::Error>;

    /// Finalize the building of `Self`.
    ///
    /// Called once the buf stream is fully consumed.
    fn build(builder: Self::Builder) -> Result<Self, Self::Error>;
}

/// Error returned from collecting into a `Vec<u8>`
#[derive(Debug)]
pub struct CollectVecError { _p: () }

impl<T: Buf> FromBufStream<T> for Vec<u8> {
    type Builder = Vec<u8>;
    type Error = CollectVecError;

    fn builder(_hint: &SizeHint) -> Vec<u8> {
        Vec::new()
    }

    fn extend(builder: &mut Self, buf: &mut T, hint: &SizeHint) -> Result<(), Self::Error> {
        let lower = hint.lower();

        // If the lower bound is greater than `usize::MAX` then we have a
        // problem
        if lower > usize::MAX as u64 {
            return Err(CollectVecError { _p: () });
        }

        let mut reserve = lower as usize;

        // If `upper` is set, use this value if it is less than or equal to 64.
        // This only really impacts the first iteration.
        match hint.upper() {
            Some(upper) if upper <= 64 => {
                reserve = upper as usize;
            }
            _ => {},
        }

        // hint.lower() represents the minimum amount of data that will be
        // received *after* this function call. We reserve this amount on top of
        // the amount of data in `buf`.
        reserve = match reserve.checked_add(buf.remaining()) {
            Some(n) => n,
            None => return Err(CollectVecError { _p: () }),
        };

        // Always reserve 64 bytes the first time, unless `upper` is set and is
        // less than 64.
        if builder.is_empty() {
            reserve = reserve.max(match hint.upper() {
                Some(upper) if upper < 64 => upper as usize,
                _ => 64,
            });
        }

        // Make sure overflow won't happen when reserving
        if reserve.checked_add(builder.len()).is_none() {
            return Err(CollectVecError { _p: () });
        }

        // Reserve space
        builder.reserve(reserve);

        // Copy the data
        builder.put(buf);

        Ok(())
    }

    fn build(builder: Self) -> Result<Self, Self::Error> {
        Ok(builder)
    }
}
