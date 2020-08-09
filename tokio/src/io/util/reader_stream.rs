use crate::io::AsyncRead;
use crate::stream::Stream;
use bytes::{Bytes, BytesMut};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Convert an [`AsyncRead`] implementor into a
    /// [`Stream`] of Result<[`Bytes`], std::io::Error>.
    /// After first error it will stop.
    /// Additionally, this stream is fused: after it returns None at some
    /// moment, it is guaranteed that further `next()`, `poll_next()` and
    /// similar functions will instantly return None.
    ///
    /// This type can be created using the [`reader_stream`] function
    ///
    /// [`AsyncRead`]: crate::io::AsyncRead
    /// [`Stream`]: crate::stream::Stream
    /// [`Bytes`]: bytes::Bytes
    /// [`reader_stream`]: crate::io::reader_stream
    #[derive(Debug)]
    #[cfg_attr(docsrs, doc(cfg(feature = "stream")))]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct ReaderStream<R> {
        // Reader itself.
        // None if we had error reading from the `reader` in the past.
        #[pin]
        reader: Option<R>,
        // Working buffer, used to optimize allocations.
        // # Capacity behavior
        // Initially `buf` is empty. Also it's getting smaller and smaller
        // during polls (because its chunks are returned to stream user).
        // But when it's capacity reaches 0, it is growed.
        buf: BytesMut,
    }
}

/// Convert an [`AsyncRead`] implementor into a
/// [`Stream`] of Result<[`Bytes`], std::io::Error>.
///
/// # Example
///
/// ```
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// use tokio::stream::StreamExt;
///
/// let data: &[u8] = b"hello, world!";
/// let mut stream = tokio::io::reader_stream(data);
/// let mut stream_contents = Vec::new();
/// while let Some(chunk) = stream.next().await {
///    stream_contents.extend_from_slice(chunk?.as_ref());
/// }
/// assert_eq!(stream_contents, data);
/// # Ok(())
/// # }
/// ```
///
/// [`AsyncRead`]: crate::io::AsyncRead
/// [`Stream`]: crate::stream::Stream
/// [`Bytes`]: bytes::Bytes
pub fn reader_stream<R>(reader: R) -> ReaderStream<R>
where
    R: AsyncRead,
{
    ReaderStream {
        reader: Some(reader),
        buf: BytesMut::new(),
    }
}

const CAPACITY: usize = 4096;

impl<R> Stream for ReaderStream<R>
where
    R: AsyncRead,
{
    type Item = std::io::Result<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();
        let reader = match this.reader.as_pin_mut() {
            Some(r) => r,
            None => return Poll::Ready(None),
        };
        if this.buf.capacity() == 0 {
            this.buf.reserve(CAPACITY);
        }
        match reader.poll_read_buf(cx, &mut this.buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => {
                self.project().reader.set(None);
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(Ok(0)) => {
                self.project().reader.set(None);
                Poll::Ready(None)
            }
            Poll::Ready(Ok(_)) => {
                let chunk = this.buf.split();
                Poll::Ready(Some(Ok(chunk.freeze())))
            }
        }
    }
}
