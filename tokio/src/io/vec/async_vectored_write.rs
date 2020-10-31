use crate::io::AsyncWrite;
use std::io::{self, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Writes bytes from a slice of buffers asynchronously.
///
/// This trait extends [`AsyncWrite`], providing
/// the functionality of [`std::io::Write::write_vectored`]
/// in a non-blocking way, and indicates that an I/O object has an efficient
/// implementation for vectored writes.
pub trait AsyncVectoredWrite: AsyncWrite {
    /// Attempt to write bytes from `slices` into the object.
    ///
    /// Data is copied from each buffer in order, with the final buffer
    /// copied from possibly being only partially consumed.
    /// This method must behave as a call to [`poll_write`]
    /// with the buffers concatenated would.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`.
    ///
    /// If the object is not ready for writing, the method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker()`) to receive a notification when the object becomes
    /// writable or is closed.
    ///
    /// [`poll_write`]: AsyncWrite::poll_write()
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        slices: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>>;
}
