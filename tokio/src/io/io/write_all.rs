use crate::io::AsyncWrite;

use bytes::Buf;
use futures_core::ready;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WriteAll<'a, W: ?Sized, B> {
    writer: &'a mut W,
    buf: B,
}

pub(crate) fn write_all<'a, W, B>(writer: &'a mut W, buf: B) -> WriteAll<'a, W, B>
where
    W: AsyncWrite + Unpin + ?Sized,
    B: Buf + Unpin,
{
    WriteAll { writer, buf }
}

impl<W, B> Future for WriteAll<'_, W, B>
where
    W: AsyncWrite + Unpin + ?Sized,
    B: Buf + Unpin,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = &mut *self;
        while me.buf.has_remaining() {
            let n = ready!(Pin::new(&mut me.writer).poll_write(cx, &mut me.buf))?;
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
        }

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<WriteAll<'_, PhantomPinned>>();
    }
}
