use crate::AsyncBufRead;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the [`read_until`](crate::io::AsyncBufReadExt::read_until) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct FillBuf<'a, R: ?Sized> {
    reader: Option<&'a mut R>,
}

pub(crate) fn fill_buf<'a, R>(reader: &'a mut R) -> FillBuf<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    FillBuf {
        reader: Some(reader),
    }
}

impl<'a, R: AsyncBufRead + ?Sized + Unpin> Future for FillBuf<'a, R> {
    type Output = io::Result<&'a [u8]>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.reader.take() {
            Some(r) => match Pin::new(&mut *r).poll_fill_buf(cx) {
                // SAFETY We either drop `self.reader` and return a slice with the lifetime of the
                // reader or we return Pending/Err (neither which contains `'a`).
                // In either case `poll_fill_buf` can not be called while it's contents are exposed
                Poll::Ready(Ok(x)) => unsafe { return Ok(&*(x as *const _)).into() },
                Poll::Ready(Err(err)) => Err(err).into(),
                Poll::Pending => {
                    self.reader = Some(r);
                    Poll::Pending
                }
            },
            None => panic!("FillBuf polled after completion"),
        }
    }
}
