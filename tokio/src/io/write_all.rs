use tokio_io::AsyncWrite;

use futures_core::ready;
use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WriteAll<'a, W: ?Sized> {
    writer: &'a mut W,
    buf: &'a [u8],
}

pub(crate) fn write_all<'a, W>(writer: &'a mut W, buf: &'a [u8]) -> WriteAll<'a, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    WriteAll { writer, buf }
}

impl<W: ?Sized + Unpin> Unpin for WriteAll<'_, W> {}

impl<W> Future for WriteAll<'_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = &mut *self;
        while !me.buf.is_empty() {
            let n = ready!(Pin::new(&mut me.writer).poll_write(cx, me.buf))?;
            {
                let (_, rest) = mem::replace(&mut me.buf, &[]).split_at(n);
                me.buf = rest;
            }
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
        }

        Poll::Ready(Ok(()))
    }
}
