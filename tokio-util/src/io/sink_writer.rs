use futures_sink::Sink;
use pin_project_lite::pin_project;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

pin_project! {
    /// Convert a [`Sink`] of byte chunks into an [`AsyncWrite`].
    ///
    /// For the inverse operation define a [`codec`].
    ///
    /// # Example
    ///
    /// ```
    /// use crate::sync::PollSender;
    /// use futures_util::SinkExt;
    /// use tokio::io::AsyncWriteExt;
    /// use std::io::{Error, ErrorKind};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Error> {
    ///    // Construct a channel pair to send data across and wrap a pollable sink.
    ///    // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
    ///    // as its error type.
    ///    let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(10);
    ///    let mut writer = SinkWriter::new(
    ///        PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::Other)),
    ///    );
    ///
    ///    // Write data to our interface...
    ///    let data: [u8; 4] = [1, 2, 3, 4];
    ///    writer.write(&data).await?;
    ///
    ///    // ... and receive it.
    ///    let mut received = Vec::new();
    ///    for _ in 0..4 {
    ///        received.push(rx.recv().await.unwrap());
    ///    }
    ///    assert_eq!(&data, received.as_slice());
    ///
    ///    # Ok(())
    /// # }
    /// ```
    ///
    ///
    /// [`AsyncWrite`]: tokio::io::AsyncWrite
    /// [`Sink`]: futures_sink::Sink
    /// [`codec`]: tokio_util::codec
    #[derive(Debug)]
    struct SinkWriter<S, T> {
        #[pin]
        inner: S,
        phantom: PhantomData<T>
    }
}
impl<S, T> SinkWriter<S, T>
where
    S: Sink<T>,
    T: From<u8>,
{
    fn new(sink: S) -> Self {
        Self {
            inner: sink,
            phantom: PhantomData,
        }
    }

    /// Gets a reference to the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Gets a mutable reference to the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut S> {
        self.project().inner
    }

    /// Consumes this `SinkWriter`, returning the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S, E, T> AsyncWrite for SinkWriter<S, T>
where
    S: Sink<T, Error = E>,
    E: Into<io::Error>,
    T: From<u8>,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let sink = self.get_pin_mut();
        match sink.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                let p = {
                    for (i, b) in buf.into_iter().enumerate() {
                        if let Err(_) = sink.start_send(From::from(*b)) {
                            return Poll::Ready(Ok(i + 1));
                        };
                    }
                    Poll::Ready(Ok(buf.len()))
                };

                sink.poll_flush(cx);
                p
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.get_pin_mut().poll_flush(cx).map_err(Into::into)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.get_pin_mut().poll_close(cx).map_err(Into::into)
    }
}
