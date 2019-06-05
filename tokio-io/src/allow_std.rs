use crate::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A simple wrapper type which allows types that only implement
/// `std::io::Read` or `std::io::Write` to be used in contexts which expect
/// an `AsyncRead` or `AsyncWrite`.
///
/// If these types issue an error with the kind `io::ErrorKind::WouldBlock`,
/// it is expected that they will notify the current task on readiness.
/// Synchronous `std` types should not issue errors of this kind and
/// are safe to use in this context. However, using these types with
/// `AllowStdIo` will cause the event loop to block, so they should be used
/// with care.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AllowStdIo<T>(T);

impl<T> AllowStdIo<T> {
    /// Creates a new `AllowStdIo` from an existing IO object.
    pub fn new(io: T) -> Self {
        AllowStdIo(io)
    }

    /// Returns a reference to the contained IO object.
    pub fn get_ref(&self) -> &T {
        &self.0
    }

    /// Returns a mutable reference to the contained IO object.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }

    /// Consumes self and returns the contained IO object.
    pub fn into_inner(self) -> T {
        self.0
    }
}

// forward Unpin
impl<T: Unpin> Unpin for AllowStdIo<T> {}

impl<T> AsyncRead for AllowStdIo<T>
where
    T: io::Read + Unpin,
{
    // TODO: override prepare_uninitialized_buffer once `Read::initializer` is stable.
    // See rust-lang/rust #42788

    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(try_nb!(self.0.read(buf))))
    }
}

impl<T> AsyncWrite for AllowStdIo<T>
where
    T: io::Write + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(try_nb!(self.0.write(buf))))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(try_nb!(self.0.flush())))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
