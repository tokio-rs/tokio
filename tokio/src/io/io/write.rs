use crate::io::AsyncWrite;

use bytes::Buf;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future to write some of the buffer to an `AsyncWrite`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Write<'a, W: ?Sized, B> {
    writer: &'a mut W,
    buf: B,
}

/// Tries to write some bytes from the given `buf` to the writer in an
/// asynchronous manner, returning a future.
pub(crate) fn write<'a, W, B>(writer: &'a mut W, buf: B) -> Write<'a, W, B>
where
    W: AsyncWrite + Unpin + ?Sized,
    B: Buf + Unpin,
{
    Write { writer, buf }
}

impl<W, B> Future for Write<'_, W, B>
where
    W: AsyncWrite + Unpin + ?Sized,
    B: Buf + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = &mut *self;
        Pin::new(&mut *me.writer).poll_write(cx, &mut me.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<Write<'_, PhantomPinned>>();
    }
}
