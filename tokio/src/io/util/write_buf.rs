use crate::io::AsyncWrite;

use bytes::Buf;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    /// A future to write some of the buffer to an `AsyncWrite`.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct WriteBuf<'a, W, B> {
        writer: &'a mut W,
        buf: &'a mut B,
    }
}

/// Tries to write some bytes from the given `buf` to the writer in an
/// asynchronous manner, returning a future.
pub(crate) fn write_buf<'a, W, B>(writer: &'a mut W, buf: &'a mut B) -> WriteBuf<'a, W, B>
where
    W: AsyncWrite,
    B: Buf,
{
    WriteBuf { writer, buf }
}

impl<W, B> Future for WriteBuf<'_, W, B>
where
    W: AsyncWrite,
    B: Buf,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        // safety: no data is moved from self
        unsafe {
            let me = self.get_unchecked_mut();
            Pin::new_unchecked(&mut *me.writer).poll_write_buf(cx, &mut me.buf)
        }
    }
}
