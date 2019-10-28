use crate::io::AsyncBufRead;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the [`read_until`](crate::io::AsyncBufReadExt::read_until) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReadIntoBuf<'a, R: ?Sized> {
    reader: &'a mut R,
}

pub(crate) fn read_into_buf<'a, R>(reader: &'a mut R) -> ReadIntoBuf<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    ReadIntoBuf {
        reader,
    }
}

impl<R: AsyncBufRead + ?Sized + Unpin> Future for ReadIntoBuf<'_, R> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.reader).poll_read_into_buf(cx)
    }
}
