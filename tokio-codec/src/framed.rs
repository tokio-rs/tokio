#![allow(deprecated)]

use crate::decoder::Decoder;
use crate::encoder::Encoder;
use crate::framed_read::{framed_read2, framed_read2_with_buffer, FramedRead2};
use crate::framed_write::{framed_write2, framed_write2_with_buffer, FramedWrite2};

use tokio_io::{AsyncRead, AsyncWrite};

use bytes::BytesMut;
use futures_core::Stream;
use futures_sink::Sink;
use std::fmt;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A unified `Stream` and `Sink` interface to an underlying I/O object, using
/// the `Encoder` and `Decoder` traits to encode and decode frames.
///
/// You can create a `Framed` instance by using the `AsyncRead::framed` adapter.
pub struct Framed<T, U> {
    inner: FramedRead2<FramedWrite2<Fuse<T, U>>>,
}

pub struct Fuse<T, U>(pub T, pub U);

impl<T, U> Framed<T, U>
where
    T: AsyncRead + AsyncWrite,
    U: Decoder + Encoder,
{
    /// Provides a `Stream` and `Sink` interface for reading and writing to this
    /// `Io` object, using `Decode` and `Encode` to read and write the raw data.
    ///
    /// Raw I/O objects work with byte sequences, but higher-level code usually
    /// wants to batch these into meaningful chunks, called "frames". This
    /// method layers framing on top of an I/O object, by using the `Codec`
    /// traits to handle encoding and decoding of messages frames. Note that
    /// the incoming and outgoing frame types may be distinct.
    ///
    /// This function returns a *single* object that is both `Stream` and
    /// `Sink`; grouping this into a single object is often useful for layering
    /// things like gzip or TLS, which require both read and write access to the
    /// underlying object.
    ///
    /// If you want to work more directly with the streams and sink, consider
    /// calling `split` on the `Framed` returned by this method, which will
    /// break them into separate objects, allowing them to interact more easily.
    pub fn new(inner: T, codec: U) -> Framed<T, U> {
        Framed {
            inner: framed_read2(framed_write2(Fuse(inner, codec))),
        }
    }
}

impl<T, U> Framed<T, U> {
    /// Provides a `Stream` and `Sink` interface for reading and writing to this
    /// `Io` object, using `Decode` and `Encode` to read and write the raw data.
    ///
    /// Raw I/O objects work with byte sequences, but higher-level code usually
    /// wants to batch these into meaningful chunks, called "frames". This
    /// method layers framing on top of an I/O object, by using the `Codec`
    /// traits to handle encoding and decoding of messages frames. Note that
    /// the incoming and outgoing frame types may be distinct.
    ///
    /// This function returns a *single* object that is both `Stream` and
    /// `Sink`; grouping this into a single object is often useful for layering
    /// things like gzip or TLS, which require both read and write access to the
    /// underlying object.
    ///
    /// This objects takes a stream and a readbuffer and a writebuffer. These field
    /// can be obtained from an existing `Framed` with the `into_parts` method.
    ///
    /// If you want to work more directly with the streams and sink, consider
    /// calling `split` on the `Framed` returned by this method, which will
    /// break them into separate objects, allowing them to interact more easily.
    pub fn from_parts(parts: FramedParts<T, U>) -> Framed<T, U> {
        Framed {
            inner: framed_read2_with_buffer(
                framed_write2_with_buffer(Fuse(parts.io, parts.codec), parts.write_buf),
                parts.read_buf,
            ),
        }
    }

    /// Returns a reference to the underlying I/O stream wrapped by
    /// `Frame`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_ref(&self) -> &T {
        &self.inner.get_ref().get_ref().0
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `Frame`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.get_mut().get_mut().0
    }

    /// Returns a reference to the underlying codec wrapped by
    /// `Frame`.
    ///
    /// Note that care should be taken to not tamper with the underlying codec
    /// as it may corrupt the stream of frames otherwise being worked with.
    pub fn codec(&self) -> &U {
        &self.inner.get_ref().get_ref().1
    }

    /// Returns a mutable reference to the underlying codec wrapped by
    /// `Frame`.
    ///
    /// Note that care should be taken to not tamper with the underlying codec
    /// as it may corrupt the stream of frames otherwise being worked with.
    pub fn codec_mut(&mut self) -> &mut U {
        &mut self.inner.get_mut().get_mut().1
    }

    /// Consumes the `Frame`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.into_inner().into_inner().0
    }

    /// Consumes the `Frame`, returning its underlying I/O stream, the buffer
    /// with unprocessed data, and the codec.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_parts(self) -> FramedParts<T, U> {
        let (inner, read_buf) = self.inner.into_parts();
        let (inner, write_buf) = inner.into_parts();

        FramedParts {
            io: inner.0,
            codec: inner.1,
            read_buf: read_buf,
            write_buf: write_buf,
            _priv: (),
        }
    }
}

impl<T, U> Stream for Framed<T, U>
where
    T: AsyncRead + Unpin,
    U: Decoder + Unpin,
{
    type Item = Result<U::Item, U::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        pin!(self.get_mut().inner).poll_next(cx)
    }
}

impl<T, I, U> Sink<I> for Framed<T, U>
where
    T: AsyncWrite + Unpin,
    U: Encoder<Item = I> + Unpin,
    U::Error: From<io::Error>,
{
    type Error = U::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(Pin::get_mut(self).inner.get_mut()).poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        Pin::new(Pin::get_mut(self).inner.get_mut()).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(Pin::get_mut(self).inner.get_mut()).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(Pin::get_mut(self).inner.get_mut()).poll_close(cx)
    }
}

impl<T, U> fmt::Debug for Framed<T, U>
where
    T: fmt::Debug,
    U: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Framed")
            .field("io", &self.inner.get_ref().get_ref().0)
            .field("codec", &self.inner.get_ref().get_ref().1)
            .finish()
    }
}

// ===== impl Fuse =====

impl<T: Read, U> Read for Fuse<T, U> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.0.read(dst)
    }
}

impl<T: AsyncRead + Unpin, U: Unpin> AsyncRead for Fuse<T, U> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.0.prepare_uninitialized_buffer(buf)
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        pin!(self.get_mut().0).poll_read(cx, buf)
    }
}

impl<T: Write, U> Write for Fuse<T, U> {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        self.0.write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<T: AsyncWrite + Unpin, U: Unpin> AsyncWrite for Fuse<T, U> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        pin!(self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        pin!(self.get_mut().0).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        pin!(self.get_mut().0).poll_shutdown(cx)
    }
}

impl<T, U: Decoder> Decoder for Fuse<T, U> {
    type Item = U::Item;
    type Error = U::Error;

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.1.decode(buffer)
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.1.decode_eof(buffer)
    }
}

impl<T, U: Encoder> Encoder for Fuse<T, U> {
    type Item = U::Item;
    type Error = U::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.1.encode(item, dst)
    }
}

/// `FramedParts` contains an export of the data of a Framed transport.
/// It can be used to construct a new `Framed` with a different codec.
/// It contains all current buffers and the inner transport.
#[derive(Debug)]
pub struct FramedParts<T, U> {
    /// The inner transport used to read bytes to and write bytes to
    pub io: T,

    /// The codec
    pub codec: U,

    /// The buffer with read but unprocessed data.
    pub read_buf: BytesMut,

    /// A buffer with unprocessed data which are not written yet.
    pub write_buf: BytesMut,

    /// This private field allows us to add additional fields in the future in a
    /// backwards compatible way.
    _priv: (),
}

impl<T, U> FramedParts<T, U> {
    /// Create a new, default, `FramedParts`
    pub fn new(io: T, codec: U) -> FramedParts<T, U> {
        FramedParts {
            io,
            codec,
            read_buf: BytesMut::new(),
            write_buf: BytesMut::new(),
            _priv: (),
        }
    }
}
