use crate::codec::encoder::Encoder;
use crate::codec::framed_impl::{FramedImpl, WriteFrame};

use futures_core::Stream;
use tokio::io::AsyncWrite;

use bytes::BytesMut;
use futures_sink::Sink;
use pin_project_lite::pin_project;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A [`Sink`] of frames encoded to an `AsyncWrite`.
    ///
    /// [`Sink`]: futures_sink::Sink
    pub struct FramedWrite<T, E> {
        #[pin]
        inner: FramedImpl<T, E, WriteFrame>,
    }
}

impl<T, E> FramedWrite<T, E>
where
    T: AsyncWrite,
{
    /// Creates a new `FramedWrite` with the given `encoder`.
    pub fn new(inner: T, encoder: E) -> FramedWrite<T, E> {
        FramedWrite {
            inner: FramedImpl {
                inner,
                codec: encoder,
                state: WriteFrame::default(),
            },
        }
    }
}

impl<T, E> FramedWrite<T, E> {
    /// Returns a reference to the underlying I/O stream wrapped by
    /// `FramedWrite`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_ref(&self) -> &T {
        &self.inner.inner
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedWrite`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.inner
    }

    /// Returns a pinned mutable reference to the underlying I/O stream wrapped by
    /// `FramedWrite`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut T> {
        self.project().inner.project().inner
    }

    /// Consumes the `FramedWrite`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.inner
    }

    /// Returns a reference to the underlying encoder.
    pub fn encoder(&self) -> &E {
        &self.inner.codec
    }

    /// Returns a mutable reference to the underlying encoder.
    pub fn encoder_mut(&mut self) -> &mut E {
        &mut self.inner.codec
    }

    /// Maps the encoder `E` to `C`, preserving the write buffer
    /// wrapped by `Framed`.
    pub fn map_encoder<C, F>(self, map: F) -> FramedWrite<T, C>
    where
        F: FnOnce(E) -> C,
    {
        // This could be potentially simplified once rust-lang/rust#86555 hits stable
        let FramedImpl {
            inner,
            state,
            codec,
        } = self.inner;
        FramedWrite {
            inner: FramedImpl {
                inner,
                state,
                codec: map(codec),
            },
        }
    }

    /// Returns a mutable reference to the underlying encoder.
    pub fn encoder_pin_mut(self: Pin<&mut Self>) -> &mut E {
        self.project().inner.project().codec
    }

    /// Returns a reference to the write buffer.
    pub fn write_buffer(&self) -> &BytesMut {
        &self.inner.state.buffer
    }

    /// Returns a mutable reference to the write buffer.
    pub fn write_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.inner.state.buffer
    }
}

// This impl just defers to the underlying FramedImpl
impl<T, I, E> Sink<I> for FramedWrite<T, E>
where
    T: AsyncWrite,
    E: Encoder<I>,
    E::Error: From<io::Error>,
{
    type Error = E::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

// This impl just defers to the underlying T: Stream
impl<T, D> Stream for FramedWrite<T, D>
where
    T: Stream,
{
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.project().inner.poll_next(cx)
    }
}

impl<T, U> fmt::Debug for FramedWrite<T, U>
where
    T: fmt::Debug,
    U: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramedWrite")
            .field("inner", &self.get_ref())
            .field("encoder", &self.encoder())
            .field("buffer", &self.inner.state.buffer)
            .finish()
    }
}
