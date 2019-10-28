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
        let mut reader = Pin::new(self.reader.take().expect("FillBuf polled after completion"));
        match reader.as_mut().poll_read_into_buf(cx) {
            Poll::Pending => {
                self.reader = Some(reader.get_mut());
                return Poll::Pending
            }
            Poll::Ready(result) => {
                result?;
            }
        }
        Poll::Ready(Ok(reader.get_buf()))
    }
}
