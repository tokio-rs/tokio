#![doc(html_root_url = "https://docs.rs/tokio-buf/0.1.0")]
#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

//! Asynchronous stream of bytes.
//!
//! This crate contains the `BufStream` trait and a number of combinators for
//! this trait. The trait is similar to `Stream` in the `futures` library, but
//! instead of yielding arbitrary values, it only yields types that implement
//! `Buf` (i.e, byte collections).

extern crate bytes;
#[cfg(feature = "ext")]
extern crate either;
#[allow(unused)]
#[macro_use]
extern crate futures;

pub mod errors;
#[cfg(feature = "ext")]
pub mod ext;
mod size_hint;
mod str;

pub use self::size_hint::SizeHint;
#[doc(inline)]
#[cfg(feature = "ext")]
pub use ext::BufStreamExt;

use bytes::{Buf, Bytes, BytesMut};
use errors::internal::Never;
use futures::Poll;
use std::io;

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

impl BufStream for Vec<u8> {
    type Item = io::Cursor<Vec<u8>>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

impl BufStream for &'static [u8] {
    type Item = io::Cursor<&'static [u8]>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

impl BufStream for Bytes {
    type Item = io::Cursor<Bytes>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

impl BufStream for BytesMut {
    type Item = io::Cursor<BytesMut>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

fn poll_bytes<T: Default>(buf: &mut T) -> Poll<Option<io::Cursor<T>>, Never> {
    use std::mem;

    let bytes = mem::replace(buf, Default::default());
    let buf = io::Cursor::new(bytes);

    Ok(Some(buf).into())
}
