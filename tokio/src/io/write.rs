use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_io::AsyncWrite;

/// A future to write some of the buffer to an `AsyncWrite`.
#[derive(Debug)]
pub struct Write<'a, W: ?Sized> {
    writer: &'a mut W,
    buf: &'a [u8],
}

/// Tries to write some bytes from the given `buf` to the writer in an
/// asynchronous manner, returning a future.
pub(crate) fn write<'a, W>(writer: &'a mut W, buf: &'a [u8]) -> Write<'a, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    Write { writer, buf }
}

// forward Unpin
impl<'a, W: Unpin + ?Sized> Unpin for Write<'_, W> {}

impl<W> Future for Write<'_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = &mut *self;
        Pin::new(&mut *me.writer).poll_write(cx, me.buf)
    }
}
