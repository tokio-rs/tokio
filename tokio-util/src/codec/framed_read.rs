use crate::codec::framed_impl::{FramedImpl, ReadFrame};
use crate::codec::Decoder;

use tokio::io::AsyncRead;
use tokio_stream::Stream;

use bytes::BytesMut;
use futures_sink::Sink;
use pin_project_lite::pin_project;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A [`Stream`] of messages decoded from an [`AsyncRead`].
    ///
    /// [`Stream`]: tokio_stream::Stream
    /// [`AsyncRead`]: tokio::io::AsyncRead
    pub struct FramedRead<T, D> {
        #[pin]
        inner: FramedImpl<T, D, ReadFrame>,
    }
}

// ===== impl FramedRead =====

impl<T, D> FramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
{
    /// Creates a new `FramedRead` with the given `decoder`.
    pub fn new(inner: T, decoder: D) -> FramedRead<T, D> {
        FramedRead {
            inner: FramedImpl {
                inner,
                codec: decoder,
                state: Default::default(),
            },
        }
    }

    /// Creates a new `FramedRead` with the given `decoder` and a buffer of `capacity`
    /// initial size.
    pub fn with_capacity(inner: T, decoder: D, capacity: usize) -> FramedRead<T, D> {
        FramedRead {
            inner: FramedImpl {
                inner,
                codec: decoder,
                state: ReadFrame {
                    eof: false,
                    is_readable: false,
                    buffer: BytesMut::with_capacity(capacity),
                },
            },
        }
    }
}

impl<T, D> FramedRead<T, D> {
    /// Returns a reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_ref(&self) -> &T {
        &self.inner.inner
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.inner
    }

    /// Returns a pinned mutable reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut T> {
        self.project().inner.project().inner
    }

    /// Consumes the `FramedRead`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.inner
    }

    /// Returns a reference to the underlying decoder.
    pub fn decoder(&self) -> &D {
        &self.inner.codec
    }

    /// Returns a mutable reference to the underlying decoder.
    pub fn decoder_mut(&mut self) -> &mut D {
        &mut self.inner.codec
    }

    /// Returns a reference to the read buffer.
    pub fn read_buffer(&self) -> &BytesMut {
        &self.inner.state.buffer
    }

    /// Returns a mutable reference to the read buffer.
    pub fn read_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.inner.state.buffer
    }
}

// This impl just defers to the underlying FramedImpl
impl<T, D> Stream for FramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
{
    type Item = Result<D::Item, D::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

// This impl just defers to the underlying T: Sink
impl<T, I, D> Sink<I> for FramedRead<T, D>
where
    T: Sink<I>,
{
    type Error = T::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        self.project().inner.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.project().inner.poll_close(cx)
    }
}

impl<T, D> fmt::Debug for FramedRead<T, D>
where
    T: fmt::Debug,
    D: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramedRead")
            .field("inner", &self.get_ref())
            .field("decoder", &self.decoder())
            .field("eof", &self.inner.state.eof)
            .field("is_readable", &self.inner.state.is_readable)
            .field("buffer", &self.read_buffer())
            .finish()
    }
}
