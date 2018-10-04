//! TODO: Dox

mod chain;
mod collect;
mod from;
mod size_hint;

pub use self::chain::Chain;
pub use self::collect::Collect;
pub use self::from::FromBufStream;
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
    /// `poll` calls.
    ///
    /// - `Ok(Async::Ready(None))` means that the stream has terminated, and
    /// `poll` should not be invoked again.
    ///
    /// # Panics
    ///
    /// Once a stream is finished, i.e. `Ready(None)` has been returned, further
    /// calls to `poll` may result in a panic or other "bad behavior".
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error>;

    /// Returns the bounds on the remaining length of the iterator.
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
    fn consume_hint(&mut self, amount: usize) {
        // By default, this function does nothing
        drop(amount);
    }

    /// Takes two buf streams and creates a new buf stream over both in
    /// sequence.
    ///
    /// `chain()` will return a new `BufStream` value which will first yield all
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
    fn collect<T>(self) -> Collect<Self, T>
    where
        Self: Sized,
        T: FromBufStream,
    {
        Collect::new(self)
    }
}
