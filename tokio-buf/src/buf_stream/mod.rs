//! Types and utilities for working with `BufStream`.

mod bytes;
mod chain;
mod collect;
pub mod errors;
mod from;
mod limit;
mod size_hint;
mod str;

pub use self::chain::Chain;
pub use self::collect::Collect;
pub use self::from::FromBufStream;
pub use self::limit::Limit;
pub use self::size_hint::SizeHint;

use bytes::Buf;
use futures::Poll;

/// An asynchronous stream of bytes.
///
/// `BufStream` asynchronously yields values implementing `Buf`, i.e. byte
/// buffers.
pub trait BufStream {
    /// Values yielded by the `BufStream`.
    ///
    /// Each item is a sequence of bytes representing a chunk of the total
    /// `ByteStream`.
    type Item: Buf;

    /// The error type this `BufStream` might generate.
    type Error;

    /// Attempt to pull out the next buffer of this stream, registering the
    /// current task for wakeup if the value is not yet available, and returning
    /// `None` if the stream is exhausted.
    ///
    /// # Return value
    ///
    /// There are several possible return values, each indicating a distinct
    /// stream state:
    ///
    /// - `Ok(Async::NotReady)` means that this stream's next value is not ready
    /// yet. Implementations will ensure that the current task will be notified
    /// when the next value may be ready.
    ///
    /// - `Ok(Async::Ready(Some(buf)))` means that the stream has successfully
    /// produced a value, `buf`, and may produce further values on subsequent
    /// `poll_buf` calls.
    ///
    /// - `Ok(Async::Ready(None))` means that the stream has terminated, and
    /// `poll_buf` should not be invoked again.
    ///
    /// # Panics
    ///
    /// Once a stream is finished, i.e. `Ready(None)` has been returned, further
    /// calls to `poll_buf` may result in a panic or other "bad behavior".
    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error>;

    /// Returns the bounds on the remaining length of the stream.
    ///
    /// The size hint allows the caller to perform certain optimizations that
    /// are dependent on the byte stream size. For example, `collect` uses the
    /// size hint to pre-allocate enough capacity to store the entirety of the
    /// data received from the byte stream.
    ///
    /// When `SizeHint::upper()` returns `Some` with a value equal to
    /// `SizeHint::lower()`, this represents the exact number of bytes that will
    /// be yielded by the `BufStream`.
    ///
    /// # Implementation notes
    ///
    /// While not enforced, implementations are expected to respect the values
    /// returned from `SizeHint`. Any deviation is considered an implementation
    /// bug. Consumers may rely on correctness in order to use the value as part
    /// of protocol impelmentations. For example, an HTTP library may use the
    /// size hint to set the `content-length` header.
    ///
    /// However, `size_hint` must not be trusted to omit bounds checks in unsafe
    /// code. An incorrect implementation of `size_hint()` must not lead to
    /// memory safety violations.
    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }

    /// Indicates to the `BufStream` how much data the consumer is currently
    /// able to process.
    ///
    /// The consume hint allows the stream to perform certain optimizations that
    /// are dependent on the consumer's readiness. For example, the consume hint
    /// may be used to request a remote peer to start sending up to `amount`
    /// data.
    ///
    /// Calling `consume_hint` is not a requirement. If `consume_hint` is never
    /// called, the stream should assume a default behavior. When `consume_hint`
    /// is called, the stream should make a best effort to honor by the request.
    ///
    /// `amount` represents the number of bytes that the caller would like to
    /// receive at the time the function is called. For example, if
    /// `consume_hint` is called with 20, the consumer requests 20 bytes. The
    /// stream may yield less than that. If the next call to `poll_buf` returns
    /// 5 bytes, the consumer still has 15 bytes requested. At this point,
    /// invoking `consume_hint` again with 20 resets the amount requested back
    /// to 20 bytes.
    ///
    /// Calling `consume_hint` with 0 as the argument informs the stream that
    /// the caller does not intend to call `poll_buf`. If `poll_buf` **is**
    /// called, the stream may, but is not obligated to, return `NotReady` even
    /// if it could produce data at that point. If it chooses to return
    /// `NotReady`, when `consume_hint` is called with a non-zero argument, the
    /// task must be notified in order to respect the `poll_buf` contract.
    fn consume_hint(&mut self, amount: usize) {
        // By default, this function does nothing
        drop(amount);
    }

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
}
