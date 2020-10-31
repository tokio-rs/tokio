use crate::io::AsyncWrite;
use std::io::{self, IoSlice};
use std::ops::DerefMut;
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

macro_rules! deref_async_vectored_write {
    () => {
        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            slices: &[IoSlice<'_>],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut **self).poll_write_vectored(cx, slices)
        }
    };
}

impl<T: ?Sized + AsyncVectoredWrite + Unpin> AsyncVectoredWrite for Box<T> {
    deref_async_vectored_write!();
}

impl<T: ?Sized + AsyncVectoredWrite + Unpin> AsyncVectoredWrite for &mut T {
    deref_async_vectored_write!();
}

impl<P> AsyncVectoredWrite for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncVectoredWrite,
{
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        slices: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().as_mut().poll_write_vectored(cx, slices)
    }
}

macro_rules! delegate_async_vectored_write_to_std {
    () => {
        #[inline]
        fn poll_write_vectored(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            slices: &[IoSlice<'_>],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(io::Write::write_vectored(self.get_mut(), slices))
        }
    };
}

impl AsyncVectoredWrite for Vec<u8> {
    delegate_async_vectored_write_to_std!();
}

impl AsyncVectoredWrite for io::Cursor<&mut [u8]> {
    delegate_async_vectored_write_to_std!();
}

impl AsyncVectoredWrite for io::Cursor<&mut Vec<u8>> {
    delegate_async_vectored_write_to_std!();
}

impl AsyncVectoredWrite for io::Cursor<Vec<u8>> {
    delegate_async_vectored_write_to_std!();
}

impl AsyncVectoredWrite for io::Cursor<Box<[u8]>> {
    delegate_async_vectored_write_to_std!();
}
