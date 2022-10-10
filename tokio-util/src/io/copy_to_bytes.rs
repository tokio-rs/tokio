use bytes::Bytes;
use futures_sink::Sink;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A helper which wraps a `Sink<&'a [u8]>` and converts it into
    /// a `Sink<Bytes>` by copying each byte slice into an owned [`Bytes`].
    ///
    /// This can be convenient when dealing with a [`SinkWriter`] which wraps a sink that
    /// needs to take ownership of the data to be sent.
    ///
    /// [`SinkWriter`]: tokio_util::io::SinkWriter
    /// [`Bytes`]: bytes::Bytes
    #[derive(Debug)]
    pub struct CopyToBytes<S> {
        #[pin]
        inner: S,
    }
}

impl<S> CopyToBytes<S> {
    /// Creates a new [`CopyToBytes`].
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// Gets a reference to the underlying sink.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Gets a mutable reference to the underlying sink.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Consumes this [`CopyToBytes`], returning the underlying sink.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<'a, S> Sink<&'a [u8]> for CopyToBytes<S>
where
    S: Sink<Bytes>,
{
    type Error = S::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.as_mut().project().inner.poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: &'a [u8]) -> Result<(), Self::Error> {
        self.as_mut()
            .project()
            .inner
            .start_send(Bytes::copy_from_slice(item))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.as_mut().project().inner.poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.as_mut().project().inner.poll_close(cx)
    }
}
