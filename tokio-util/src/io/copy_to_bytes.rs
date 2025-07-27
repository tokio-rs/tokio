use bytes::{BufMut, Bytes, BytesMut};
use futures_core::stream::Stream;
use futures_sink::Sink;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A helper that wraps a [`Sink`]`<`[`Bytes`]`>` and converts it into a
    /// [`Sink`]`<&'a [u8]>` by copying each byte slice into an owned [`Bytes`].
    ///
    /// See the documentation for [`SinkWriter`] for an example.
    ///
    /// [`Bytes`]: bytes::Bytes
    /// [`SinkWriter`]: crate::io::SinkWriter
    /// [`Sink`]: futures_sink::Sink
    #[derive(Debug)]
    pub struct CopyToBytes<S> {
        #[pin]
        inner: S,
        buf: Option<BytesMut>
    }
}

impl<S> CopyToBytes<S> {
    /// Creates a new [`CopyToBytes`].
    pub fn new(inner: S) -> Self {
        Self { inner, buf: None }
    }

    /// Creates a new [`CopyToBytes`] with an internal buffer of given capacity.
    pub fn with_capacity(inner: S, capacity: usize) -> Self {
        Self {
            inner,
            buf: Some(BytesMut::with_capacity(capacity)),
        }
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

    fn flush_buf(mut self: Pin<&mut Self>) -> Result<(), S::Error>
    where
        S: Sink<Bytes>,
    {
        let this = self.as_mut().project();
        if let Some(buf) = this.buf {
            if !buf.is_empty() {
                let chunk = buf.split().freeze();
                this.inner.start_send(chunk)?;
            }
        }
        Ok(())
    }
}

impl<'a, S> Sink<&'a [u8]> for CopyToBytes<S>
where
    S: Sink<Bytes>,
{
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: &'a [u8]) -> Result<(), Self::Error> {
        let mut this = self.as_mut().project();

        if let Some(buf) = this.buf {
            if buf.len() + item.len() > buf.capacity() {
                drop(this);
                self.as_mut().flush_buf()?;
                this = self.as_mut().project();
            }

            this.buf.as_mut().unwrap().put(item);
            Ok(())
        } else {
            this.inner.start_send(Bytes::copy_from_slice(item))
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.as_mut().flush_buf()?;
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.as_mut().flush_buf()?;
        self.project().inner.poll_close(cx)
    }
}

impl<S: Stream> Stream for CopyToBytes<S> {
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}
