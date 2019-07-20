use super::framed::Fuse;
use super::Decoder;

use tokio_io::AsyncRead;

use bytes::BytesMut;
use futures_core::Stream;
use futures_sink::Sink;
use log::trace;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A `Stream` of messages decoded from an `AsyncRead`.
pub struct FramedRead<T, D> {
    inner: FramedRead2<Fuse<T, D>>,
}

pub struct FramedRead2<T> {
    inner: T,
    eof: bool,
    is_readable: bool,
    buffer: BytesMut,
}

const INITIAL_CAPACITY: usize = 8 * 1024;

// ===== impl FramedRead =====

impl<T, D> FramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
{
    /// Creates a new `FramedRead` with the given `decoder`.
    pub fn new(inner: T, decoder: D) -> FramedRead<T, D> {
        FramedRead {
            inner: framed_read2(Fuse(inner, decoder)),
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
        &self.inner.inner.0
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.inner.0
    }

    /// Consumes the `FramedRead`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.inner.0
    }

    /// Returns a reference to the underlying decoder.
    pub fn decoder(&self) -> &D {
        &self.inner.inner.1
    }

    /// Returns a mutable reference to the underlying decoder.
    pub fn decoder_mut(&mut self) -> &mut D {
        &mut self.inner.inner.1
    }
}

impl<T, D> Stream for FramedRead<T, D>
where
    T: AsyncRead + Unpin,
    D: Decoder + Unpin,
{
    type Item = Result<D::Item, D::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        pin!(self.get_mut().inner).poll_next(cx)
    }
}

// This impl just defers to the underlying T: Sink
impl<T, I, D> Sink<I> for FramedRead<T, D>
where
    T: Sink<I> + Unpin,
    D: Unpin,
{
    type Error = T::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(Pin::get_mut(self).inner.inner.0).poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        pin!(Pin::get_mut(self).inner.inner.0).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(Pin::get_mut(self).inner.inner.0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(Pin::get_mut(self).inner.inner.0).poll_close(cx)
    }
}

impl<T, D> fmt::Debug for FramedRead<T, D>
where
    T: fmt::Debug,
    D: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramedRead")
            .field("inner", &self.inner.inner.0)
            .field("decoder", &self.inner.inner.1)
            .field("eof", &self.inner.eof)
            .field("is_readable", &self.inner.is_readable)
            .field("buffer", &self.inner.buffer)
            .finish()
    }
}

// ===== impl FramedRead2 =====

pub fn framed_read2<T>(inner: T) -> FramedRead2<T> {
    FramedRead2 {
        inner,
        eof: false,
        is_readable: false,
        buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
    }
}

pub fn framed_read2_with_buffer<T>(inner: T, mut buf: BytesMut) -> FramedRead2<T> {
    if buf.capacity() < INITIAL_CAPACITY {
        let bytes_to_reserve = INITIAL_CAPACITY - buf.capacity();
        buf.reserve(bytes_to_reserve);
    }
    FramedRead2 {
        inner,
        eof: false,
        is_readable: !buf.is_empty(),
        buffer: buf,
    }
}

impl<T> FramedRead2<T> {
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

impl<T> Stream for FramedRead2<T>
where
    T: AsyncRead + Decoder + Unpin,
{
    type Item = Result<T::Item, T::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pinned = Pin::get_mut(self);
        loop {
            // Repeatedly call `decode` or `decode_eof` as long as it is
            // "readable". Readable is defined as not having returned `None`. If
            // the upstream has returned EOF, and the decoder is no longer
            // readable, it can be assumed that the decoder will never become
            // readable again, at which point the stream is terminated.
            if pinned.is_readable {
                if pinned.eof {
                    let frame = pinned.inner.decode_eof(&mut pinned.buffer)?;
                    return Poll::Ready(frame.map(Ok));
                }

                trace!("attempting to decode a frame");

                if let Some(frame) = pinned.inner.decode(&mut pinned.buffer)? {
                    trace!("frame decoded from buffer");
                    return Poll::Ready(Some(Ok(frame)));
                }

                pinned.is_readable = false;
            }

            assert!(!pinned.eof);

            // Otherwise, try to read more data and try again. Make sure we've
            // got room for at least one byte to read to ensure that we don't
            // get a spurious 0 that looks like EOF
            pinned.buffer.reserve(1);
            let bytect = match pin!(pinned.inner).poll_read_buf(cx, &mut pinned.buffer)? {
                Poll::Ready(ct) => ct,
                Poll::Pending => return Poll::Pending,
            };
            if bytect == 0 {
                pinned.eof = true;
            }

            pinned.is_readable = true;
        }
    }
}
