use futures_sink::Sink;

use pin_project_lite::pin_project;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

pin_project! {
    /// Convert a [`Sink`] of byte chunks into an [`AsyncWrite`].
    ///
    /// Whenever you write to this [`SinkWriter`], the supplied bytes are
    /// forwarded to the inner [`Sink`]. When [`shutdown`] is called on this
    /// [`SinkWriter`], the inner sink is closed.
    ///
    /// This adapter implements [`AsyncWrite`] for `Sink<&[u8]>`. If you want to
    /// implement `Sink<_>` for [`AsyncWrite`], see the [`codec`] module; if you need to implement
    /// [`AsyncWrite`] for `Sink<Bytes>`, see [`CopyToBytes`].
    ///
    /// # Example
    ///
    /// ```
    /// use bytes::Bytes;
    /// use futures_util::SinkExt;
    /// use std::io::{Error, ErrorKind};
    /// use tokio::io::AsyncWriteExt;
    /// use tokio_util::io::{SinkWriter, CopyToBytes};
    /// use tokio_util::sync::PollSender;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Error> {
    /// // Construct a channel pair to send data across and wrap a pollable sink.
    /// // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
    /// // as its error type.
    /// let (tx, mut rx) = tokio::sync::mpsc::channel::<Bytes>(1);
    /// let mut writer = SinkWriter::new(CopyToBytes::new(
    ///   PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::BrokenPipe)),
    /// ));
    ///
    /// // Write data to our interface...
    /// let data: [u8; 4] = [1, 2, 3, 4];
    /// let _ = writer.write(&data).await?;
    ///
    /// // ... and receive it.
    /// assert_eq!(data.to_vec(), rx.recv().await.unwrap().to_vec());
    /// # Ok(())
    /// # }
    /// ```
    ///
    ///
    /// [`AsyncWrite`]: tokio::io::AsyncWrite
    /// [`Sink`]: futures_sink::Sink
    /// [`codec`]: tokio_util::codec
    /// [`CopyToBytes`]: tokio_util::io::CopyToBytes
    #[derive(Debug)]
    pub struct SinkWriter<S> {
        #[pin]
        inner: S,
    }
}

impl<S> SinkWriter<S> {
    /// Creates a new [`SinkWriter`].
    pub fn new(sink: S) -> Self {
        Self { inner: sink }
    }

    /// Gets a reference to the underlying sink.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Gets a mutable reference to the underlying sink.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Consumes this [`SinkWriter`], returning the underlying sink.
    pub fn into_inner(self) -> S {
        self.inner
    }
}
impl<S, E> AsyncWrite for SinkWriter<S>
where
    for<'a> S: Sink<&'a [u8], Error = E>,
    E: Into<io::Error>,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.as_mut().project().inner.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                if let Err(e) = self.as_mut().project().inner.start_send(buf) {
                    Poll::Ready(Err(e.into()))
                } else {
                    Poll::Ready(Ok(buf.len()))
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_flush(cx).map_err(Into::into)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_close(cx).map_err(Into::into)
    }
}
