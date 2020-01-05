use crate::io::{AsyncBufRead, AsyncRead};
use crate::stream::Stream;
use bytes::{BufMut, Bytes};
use pin_project_lite::pin_project;
use std::io;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Convert a stream of byte chunks into an [`AsyncRead`](crate::io::AsyncRead).
    ///
    /// # Example
    ///
    /// ```
    /// use bytes::Bytes;
    /// use tokio::stream::{iter, IntoAsyncRead};
    /// use tokio::io::AsyncReadExt;
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///
    /// let stream = iter(vec![
    ///     Ok(Bytes::from_static(&[0, 1, 2, 3])),
    ///     Ok(Bytes::from_static(&[4, 5, 6, 7])),
    ///     Ok(Bytes::from_static(&[8, 9, 10, 11])),
    /// ]);
    ///
    /// let mut read = IntoAsyncRead::new(stream);
    ///
    /// // Read five bytes from the stream:
    /// let mut buf = [0; 5];
    /// read.read_exact(&mut buf).await?;
    /// assert_eq!(buf, [0, 1, 2, 3, 4]);
    ///
    /// // Read the rest of the current chunk.
    /// assert_eq!(read.read(&mut buf).await?, 3);
    /// assert_eq!(&buf[..3], [5, 6, 7]);
    ///
    /// // Read the next chunk.
    /// assert_eq!(read.read(&mut buf).await?, 4);
    /// assert_eq!(&buf[..4], [8, 9, 10, 11]);
    ///
    /// // We have now reached the end.
    /// assert_eq!(read.read(&mut buf).await?, 0);
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[derive(Debug)]
    pub struct IntoAsyncRead<S> {
        #[pin]
        inner: S,
        chunk: Bytes,
    }
}

impl<S> IntoAsyncRead<S>
where
    S: Stream<Item = Result<Bytes, io::Error>>,
{
    /// Convert the provided stream into an [`AsyncRead`](crate::io::AsyncRead).
    pub fn new(stream: S) -> Self {
        IntoAsyncRead {
            inner: stream,
            chunk: Bytes::new(),
        }
    }
}

impl<S> AsyncRead for IntoAsyncRead<S>
where
    S: Stream<Item = Result<Bytes, io::Error>>,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let inner_buf = match self.as_mut().poll_fill_buf(cx) {
            Poll::Ready(Ok(buf)) => buf,
            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
            Poll::Pending => return Poll::Pending,
        };
        let len = std::cmp::min(inner_buf.len(), buf.len());
        (&mut buf[..len]).copy_from_slice(&inner_buf[..len]);

        self.consume(len);
        Poll::Ready(Ok(len))
    }
    fn poll_read_buf<B: BufMut>(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>>
    where
        Self: Sized,
    {
        if !buf.has_remaining_mut() {
            return Poll::Ready(Ok(0));
        }

        let inner_buf = match self.as_mut().poll_fill_buf(cx) {
            Poll::Ready(Ok(buf)) => buf,
            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
            Poll::Pending => return Poll::Pending,
        };
        let len = std::cmp::min(inner_buf.len(), buf.remaining_mut());
        buf.put_slice(&inner_buf[..len]);

        self.consume(len);
        Poll::Ready(Ok(len))
    }
    unsafe fn prepare_uninitialized_buffer(&self, _buf: &mut [MaybeUninit<u8>]) -> bool {
        false
    }
}

impl<S> AsyncBufRead for IntoAsyncRead<S>
where
    S: Stream<Item = Result<Bytes, io::Error>>,
{
    fn poll_fill_buf(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let brw_project = self.as_mut().project();
        if brw_project.chunk.is_empty() {
            match brw_project.inner.poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    *brw_project.chunk = chunk;
                    Poll::Ready(Ok(&*self.project().chunk))
                }
                Poll::Ready(Some(Err(err))) => Poll::Ready(Err(err)),
                Poll::Ready(None) => Poll::Ready(Ok(&[])),
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(&*self.project().chunk))
        }
    }
    fn consume(self: Pin<&mut Self>, amt: usize) {
        let _ = self.project().chunk.split_to(amt);
    }
}
