use bytes::Buf;
use futures_core::stream::Stream;
use pin_project_lite::pin_project;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

pin_project! {
    /// Convert a [`Stream`] of byte chunks into an [`AsyncRead`].
    ///
    /// This type performs the inverse operation of [`ReaderStream`].
    ///
    /// # Example
    ///
    /// ```
    /// use bytes::Bytes;
    /// use tokio::io::{AsyncReadExt, Result};
    /// use tokio_util::io::StreamReader;
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///
    /// // Create a stream from an iterator.
    /// let stream = tokio_stream::iter(vec![
    ///     Result::Ok(Bytes::from_static(&[0, 1, 2, 3])),
    ///     Result::Ok(Bytes::from_static(&[4, 5, 6, 7])),
    ///     Result::Ok(Bytes::from_static(&[8, 9, 10, 11])),
    /// ]);
    ///
    /// // Convert it to an AsyncRead.
    /// let mut read = StreamReader::new(stream);
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
    ///
    /// [`AsyncRead`]: tokio::io::AsyncRead
    /// [`Stream`]: futures_core::Stream
    /// [`ReaderStream`]: crate::io::ReaderStream
    #[derive(Debug)]
    pub struct StreamReader<S, B> {
        #[pin]
        inner: S,
        chunk: Option<B>,
    }
}

impl<S, B, E> StreamReader<S, B>
where
    S: Stream<Item = Result<B, E>>,
    B: Buf,
    E: Into<std::io::Error>,
{
    /// Convert a stream of byte chunks into an [`AsyncRead`](tokio::io::AsyncRead).
    ///
    /// The item should be a [`Result`] with the ok variant being something that
    /// implements the [`Buf`] trait (e.g. `Vec<u8>` or `Bytes`). The error
    /// should be convertible into an [io error].
    ///
    /// [`Result`]: std::result::Result
    /// [`Buf`]: bytes::Buf
    /// [io error]: std::io::Error
    pub fn new(stream: S) -> Self {
        Self {
            inner: stream,
            chunk: None,
        }
    }

    /// Do we have a chunk and is it non-empty?
    fn has_chunk(&self) -> bool {
        if let Some(ref chunk) = self.chunk {
            chunk.remaining() > 0
        } else {
            false
        }
    }

    /// Consumes this `StreamReader`, returning a Tuple consisting
    /// of the underlying stream and an Option of the interal buffer,
    /// which is Some in case the buffer contains elements.
    pub fn into_inner_with_chunk(self) -> (S, Option<B>) {
        if self.has_chunk() {
            (self.inner, self.chunk)
        } else {
            (self.inner, None)
        }
    }
}

impl<S, B> StreamReader<S, B> {
    /// Gets a reference to the underlying stream.
    ///
    /// It is inadvisable to directly read from the underlying stream.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Gets a mutable reference to the underlying stream.
    ///
    /// It is inadvisable to directly read from the underlying stream.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying stream.
    ///
    /// It is inadvisable to directly read from the underlying stream.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut S> {
        self.project().inner
    }

    /// Consumes this `BufWriter`, returning the underlying stream.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    /// If you additionally want access to the internal buffer use
    /// [`into_inner_with_chunk`].
    ///
    /// [`into_inner_with_chunk`]: crate::io::StreamReader::into_inner_with_chunk
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S, B, E> AsyncRead for StreamReader<S, B>
where
    S: Stream<Item = Result<B, E>>,
    B: Buf,
    E: Into<std::io::Error>,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let inner_buf = match self.as_mut().poll_fill_buf(cx) {
            Poll::Ready(Ok(buf)) => buf,
            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
            Poll::Pending => return Poll::Pending,
        };
        let len = std::cmp::min(inner_buf.len(), buf.remaining());
        buf.put_slice(&inner_buf[..len]);

        self.consume(len);
        Poll::Ready(Ok(()))
    }
}

impl<S, B, E> AsyncBufRead for StreamReader<S, B>
where
    S: Stream<Item = Result<B, E>>,
    B: Buf,
    E: Into<std::io::Error>,
{
    fn poll_fill_buf(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        loop {
            if self.as_mut().has_chunk() {
                // This unwrap is very sad, but it can't be avoided.
                let buf = self.project().chunk.as_ref().unwrap().chunk();
                return Poll::Ready(Ok(buf));
            } else {
                match self.as_mut().project().inner.poll_next(cx) {
                    Poll::Ready(Some(Ok(chunk))) => {
                        // Go around the loop in case the chunk is empty.
                        *self.as_mut().project().chunk = Some(chunk);
                    }
                    Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(err.into())),
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
