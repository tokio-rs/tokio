#![allow(deprecated)]

use super::framed::Fuse;
use crate::decoder::Decoder;
use crate::encoder::Encoder;

use tokio_io::{AsyncRead, AsyncWrite};

use bytes::BytesMut;
use futures_core::{ready, Stream};
use futures_sink::Sink;
use log::trace;
use std::fmt;
use std::io::{self, Read};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A `Sink` of frames encoded to an `AsyncWrite`.
pub struct FramedWrite<T, E> {
    inner: FramedWrite2<Fuse<T, E>>,
}

pub struct FramedWrite2<T> {
    inner: T,
    buffer: BytesMut,
}

const INITIAL_CAPACITY: usize = 8 * 1024;
const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

impl<T, E> FramedWrite<T, E>
where
    T: AsyncWrite,
    E: Encoder,
{
    /// Creates a new `FramedWrite` with the given `encoder`.
    pub fn new(inner: T, encoder: E) -> FramedWrite<T, E> {
        FramedWrite {
            inner: framed_write2(Fuse(inner, encoder)),
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
        &self.inner.inner.0
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedWrite`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.inner.0
    }

    /// Consumes the `FramedWrite`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.inner.0
    }

    /// Returns a reference to the underlying decoder.
    pub fn encoder(&self) -> &E {
        &self.inner.inner.1
    }

    /// Returns a mutable reference to the underlying decoder.
    pub fn encoder_mut(&mut self) -> &mut E {
        &mut self.inner.inner.1
    }
}

// This impl just defers to the underlying FramedWrite2
impl<T, I, E> Sink<I> for FramedWrite<T, E>
where
    T: AsyncWrite + Unpin,
    E: Encoder<Item = I> + Unpin,
    E::Error: From<io::Error>,
{
    type Error = E::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(Pin::get_mut(self).inner).poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        pin!(Pin::get_mut(self).inner).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(Pin::get_mut(self).inner).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(Pin::get_mut(self).inner).poll_close(cx)
    }
}

impl<T, D> Stream for FramedWrite<T, D>
where
    T: Stream + Unpin,
    D: Unpin,
{
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(Pin::get_mut(self).get_mut()).poll_next(cx)
    }
}

impl<T, U> fmt::Debug for FramedWrite<T, U>
where
    T: fmt::Debug,
    U: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramedWrite")
            .field("inner", &self.inner.get_ref().0)
            .field("encoder", &self.inner.get_ref().1)
            .field("buffer", &self.inner.buffer)
            .finish()
    }
}

// ===== impl FramedWrite2 =====

pub fn framed_write2<T>(inner: T) -> FramedWrite2<T> {
    FramedWrite2 {
        inner: inner,
        buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
    }
}

pub fn framed_write2_with_buffer<T>(inner: T, mut buf: BytesMut) -> FramedWrite2<T> {
    if buf.capacity() < INITIAL_CAPACITY {
        let bytes_to_reserve = INITIAL_CAPACITY - buf.capacity();
        buf.reserve(bytes_to_reserve);
    }
    FramedWrite2 {
        inner: inner,
        buffer: buf,
    }
}

impl<T> FramedWrite2<T> {
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn into_parts(self) -> (T, BytesMut) {
        (self.inner, self.buffer)
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<I, T> Sink<I> for FramedWrite2<T>
where
    T: AsyncWrite + Encoder<Item = I> + Unpin,
{
    type Error = T::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
            match self.as_mut().poll_flush(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => (),
            };

            if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
                return Poll::Pending;
            }
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let pinned = Pin::get_mut(self);
        pinned.inner.encode(item, &mut pinned.buffer)?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        trace!("flushing framed transport");
        let pinned = Pin::get_mut(self);

        while !pinned.buffer.is_empty() {
            trace!("writing; remaining={}", pinned.buffer.len());

            let buf = &pinned.buffer;
            let n = ready!(pin!(pinned.inner).poll_write(cx, &buf))?;

            if n == 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to \
                     write frame to transport",
                )
                .into()));
            }

            // TODO: Add a way to `bytes` to do this w/o returning the drained data.
            let _ = pinned.buffer.split_to(n);
        }

        // Try flushing the underlying IO
        ready!(pin!(pinned.inner).poll_flush(cx))?;

        trace!("framed transport flushed");
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(pin!(self).poll_flush(cx))?;
        ready!(pin!(self.inner).poll_shutdown(cx))?;

        Poll::Ready(Ok(()))
    }
}

impl<T: Decoder> Decoder for FramedWrite2<T> {
    type Item = T::Item;
    type Error = T::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<T::Item>, T::Error> {
        self.inner.decode(src)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<T::Item>, T::Error> {
        self.inner.decode_eof(src)
    }
}

impl<T: Read> Read for FramedWrite2<T> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.inner.read(dst)
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for FramedWrite2<T> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.inner.prepare_uninitialized_buffer(buf)
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        pin!(Pin::get_mut(self).inner).poll_read(cx, buf)
    }
}
