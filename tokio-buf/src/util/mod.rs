//! Types and utilities for working with `BufStream`.

mod chain;
mod collect;
mod from;
mod iter;
mod limit;
mod stream;

pub use self::chain::Chain;
pub use self::collect::Collect;
pub use self::from::FromBufStream;
pub use self::iter::iter;
pub use self::limit::Limit;
pub use self::stream::{stream, IntoStream};

pub mod error {
    //! Error types

    pub use super::collect::CollectError;
    pub use super::from::{CollectBytesError, CollectVecError};
    pub use super::limit::LimitError;
}

use BufStream;

impl<T> BufStreamExt for T where T: BufStream {}

/// An extension trait for `BufStream`'s that provides a variety of convenient
/// adapters.
pub trait BufStreamExt: BufStream {
    /// Takes two buf streams and creates a new buf stream over both in
    /// sequence.
    ///
    /// `chain()` returns a new `BufStream` value which will first yield all
    /// data from `self` then all data from `other`.
    ///
    /// In other words, it links two buf streams together, in a chain.
    fn chain<T>(self, other: T) -> Chain<Self, T>
    where
        Self: Sized,
        T: BufStream<Error = Self::Error>,
    {
        Chain::new(self, other)
    }

    /// Consumes all data from `self`, storing it in byte storage of type `T`.
    ///
    /// `collect()` returns a future that buffers all data yielded from `self`
    /// into storage of type of `T`. The future completes once `self` yield
    /// `None`, returning the buffered data.
    ///
    /// The collect future will yield an error if `self` yields an error or if
    /// the collect operation errors. The collect error cases are dependent on
    /// the target storage type.
    fn collect<T>(self) -> Collect<Self, T>
    where
        Self: Sized,
        T: FromBufStream<Self::Item>,
    {
        Collect::new(self)
    }

    /// Limit the number of bytes that the stream can yield.
    ///
    /// `limit()` returns a new `BufStream` value which yields all the data from
    /// `self` while ensuring that at most `amount` bytes are yielded.
    ///
    /// If `self` can yield greater than `amount` bytes, the returned stream
    /// will yield an error.
    fn limit(self, amount: u64) -> Limit<Self>
    where
        Self: Sized,
    {
        Limit::new(self, amount)
    }

    /// Creates a `Stream` from a `BufStream`.
    ///
    /// This produces a `Stream` of `BufStream::Items`.
    fn into_stream(self) -> IntoStream<Self>
    where
        Self: Sized,
    {
        IntoStream::new(self)
    }
}
