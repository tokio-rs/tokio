use crate::io::{AsyncBufRead, AsyncRead};
use crate::stream::Stream;
use bytes::{Buf, BufMut};
use pin_project_lite::pin_project;
use std::io;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Convert a stream of byte chunks into an [`AsyncRead`].
    ///
    /// This type is usually created using the [`stream_reader`] function.
    ///
    /// [`AsyncRead`]: crate::io::AsyncRead
    /// [`stream_reader`]: crate::io::stream_reader
    #[derive(Debug)]
    #[cfg_attr(docsrs, doc(cfg(feature = "stream")))]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct StreamReader<S, B> {
        #[pin]
        inner: S,
        chunk: Option<B>,
    }
}

/// Convert a stream of byte chunks into an [`AsyncRead`](crate::io::AsyncRead).
///
/// # Example
///
/// ```
/// use bytes::Bytes;
/// use tokio::io::{stream_reader, AsyncReadExt};
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
///
/// // Create a stream from an iterator.
/// let stream = tokio::stream::iter(vec![
///     Ok(Bytes::from_static(&[0, 1, 2, 3])),
///     Ok(Bytes::from_static(&[4, 5, 6, 7])),
///     Ok(Bytes::from_static(&[8, 9, 10, 11])),
/// ]);
///
/// // Convert it to an AsyncRead.
/// let mut read = stream_reader(stream);
///
/// // Read five bytes from the stream.
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
#[cfg_attr(docsrs, doc(cfg(feature = "stream")))]
#[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
pub fn stream_reader<S, B>(stream: S) -> StreamReader<S, B>
where
    S: Stream<Item = Result<B, io::Error>>,
    B: Buf,
{
    StreamReader::new(stream)
}

impl<S, B> StreamReader<S, B>
where
    S: Stream<Item = Result<B, io::Error>>,
    B: Buf,
{
    /// Convert the provided stream into an `AsyncRead`.
    fn new(stream: S) -> Self {
        Self {
            inner: stream,
            chunk: None,
        }
    }
    /// Do we have a chunk and is it non-empty?
    fn has_chunk(self: Pin<&mut Self>) -> bool {
        if let Some(chunk) = self.project().chunk {
            chunk.remaining() > 0
        } else {
            false
        }
    }
}

impl<S, B> AsyncRead for StreamReader<S, B>
where
    S: Stream<Item = Result<B, io::Error>>,
    B: Buf,
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
    fn poll_read_buf<BM: BufMut>(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut BM,
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

impl<S, B> AsyncBufRead for StreamReader<S, B>
where
    S: Stream<Item = Result<B, io::Error>>,
    B: Buf,
{
    fn poll_fill_buf(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        loop {
            if self.as_mut().has_chunk() {
                // This unwrap is very sad, but it can't be avoided.
                let buf = self.project().chunk.as_ref().unwrap().bytes();
                return Poll::Ready(Ok(buf));
            } else {
                match self.as_mut().project().inner.poll_next(cx) {
                    Poll::Ready(Some(Ok(chunk))) => {
                        // Go around the loop in case the chunk is empty.
                        *self.as_mut().project().chunk = Some(chunk);
                    }
                    Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(err)),
                    Poll::Ready(None) => return Poll::Ready(Ok(&[])),
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
    }
    fn consume(self: Pin<&mut Self>, amt: usize) {
        if amt > 0 {
            self.project()
                .chunk
                .as_mut()
                .expect("No chunk present")
                .advance(amt);
        }
    }
}
