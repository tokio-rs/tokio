#![doc(html_root_url = "https://docs.rs/tokio-buf/0.1.1")]
#![deny(missing_docs, missing_debug_implementations, unreachable_pub)]
#![cfg_attr(test, deny(warnings))]

//! Asynchronous stream of bytes.
//!
//! This crate contains the `BufStream` trait and a number of combinators for
//! this trait. The trait is similar to `Stream` in the `futures` library, but
//! instead of yielding arbitrary values, it only yields types that implement
//! `Buf` (i.e, byte collections).

extern crate bytes;
#[cfg(feature = "util")]
extern crate either;
#[allow(unused)]
#[macro_use]
extern crate futures;

mod never;
mod size_hint;
mod str;
mod u8;
#[cfg(feature = "util")]
pub mod util;

pub use self::size_hint::SizeHint;
#[doc(inline)]
#[cfg(feature = "util")]
pub use util::BufStreamExt;

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
}
