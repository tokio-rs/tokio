use crate::io::AsyncRead;
use crate::stream::Stream;
use bytes::{Bytes, BytesMut};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Convert async reader into stream of Result<Bytes, io::Error>
    ///
    /// This type can be created using the [`reader_stream`](crate::io::reader_stream) function
    #[derive(Debug)]
    #[cfg_attr(docsrs, doc(cfg(feature = "stream")))]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct ReaderStream<R> {
        // reader itself
        #[pin]
        reader: R,
        // Working buffer, used to optimize allocations.
        // # Capacity behavior
        // Initially `buf` is empty. Also it's getting smaller and smaller
        // during polls (because it's chunks are returned to stream user).
        // But when it's capacity reaches 0, it is growed.
        buf: BytesMut,
    }
}

/// Convert `AsyncRead` into stream of byte chunks.
/// # Example
/// ```
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// use tokio::stream::StreamExt;    
/// let data = b"hello, world!";
/// let mut stream = tokio::io::reader_stream(data as &[u8]);
/// let mut stream_contents = bytes::BytesMut::new();
/// while let Some(chunk) = stream.next().await {
///    stream_contents.extend_from_slice(chunk?.as_ref());
/// }
/// assert_eq!(stream_contents.as_ref(), data);
/// # Ok(())
/// # }
/// ```
pub fn reader_stream<R>(reader: R) -> ReaderStream<R>
where
    R: AsyncRead,
{
    ReaderStream {
        reader,
        buf: BytesMut::new(),
    }
}

const CAPACITY: usize = 4096;

impl<R> Stream for ReaderStream<R>
where
    R: AsyncRead,
{
    type Item = std::io::Result<Bytes>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if this.buf.capacity() == 0 {
            this.buf.reserve(CAPACITY);
        }
        // if we have something in our buf, let's return it
        match this.reader.poll_read_buf(cx, &mut this.buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err))),
            Poll::Ready(Ok(0)) => Poll::Ready(None),
            Poll::Ready(Ok(n)) => {
                let chunk = this.buf.split_to(n);
                Poll::Ready(Some(Ok(chunk.freeze())))
            }
        }
    }
}
