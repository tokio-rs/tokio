use crate::codec::{Decoder, Encoder, RWFrames, ReadFrame, WriteFrame};
use crate::udp::framed_impl::UdpFramedImpl;

use pin_project_lite::pin_project;
use tokio::net::UdpSocket;
use tokio_stream::Stream;

use bytes::BytesMut;
use futures_sink::Sink;
use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    pin::Pin,
    task::{Context, Poll},
};

pin_project! {
    /// A unified [`Stream`] and [`Sink`] interface to an underlying `UdpSocket`, using
    /// the `Encoder` and `Decoder` traits to encode and decode frames.
    ///
    /// Raw UDP sockets work with datagrams, but higher-level code usually wants to
    /// batch these into meaningful chunks, called "frames". This method layers
    /// framing on top of this socket by using the `Encoder` and `Decoder` traits to
    /// handle encoding and decoding of messages frames. Note that the incoming and
    /// outgoing frame types may be distinct.
    ///
    /// This function returns a *single* object that is both [`Stream`] and [`Sink`];
    /// grouping this into a single object is often useful for layering things which
    /// require both read and write access to the underlying object.
    ///
    /// If you want to work more directly with the streams and sink, consider
    /// calling [`split`] on the `UdpFramed` returned by this method, which will break
    /// them into separate objects, allowing them to interact more easily.
    ///
    /// [`Stream`]: tokio::stream::Stream
    /// [`Sink`]: futures_sink::Sink
    /// [`split`]: https://docs.rs/futures/0.3/futures/stream/trait.StreamExt.html#method.split.
    #[cfg_attr(docsrs, doc(all(feature = "codec", feature = "udp")))]
    pub struct UdpFramed<C> {
        #[pin]
        inner: UdpFramedImpl<C, RWFrames>,
    }
}

impl<C> Stream for UdpFramed<C>
where
    C: Decoder,
{
    type Item = Result<(C::Item, SocketAddr), C::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

// This impl just defers to the underlying FramedImpl
impl<I, C> Sink<(I, SocketAddr)> for UdpFramed<C>
where
    C: Encoder<I>,
    C::Error: From<io::Error>,
{
    type Error = C::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: (I, SocketAddr)) -> Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

impl<C> UdpFramed<C> {
    /// Create a new `UdpFramed` backed by the given socket and codec.
    ///
    /// See struct level documentation for more details.
    pub fn new(socket: UdpSocket, codec: C) -> UdpFramed<C> {
        Self {
            inner: UdpFramedImpl {
                inner: socket,
                codec,
                state: RWFrames {
                    read: ReadFrame {
                        buffer: BytesMut::with_capacity(
                            crate::udp::framed_impl::INITIAL_RD_CAPACITY,
                        ),
                        ..ReadFrame::default()
                    },
                    write: WriteFrame {
                        buffer: BytesMut::with_capacity(
                            crate::udp::framed_impl::INITIAL_WR_CAPACITY,
                        ),
                    },
                },
                out_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
                flushed: true,
                current_addr: None,
            },
        }
    }

    /// Returns a reference to the underlying I/O stream wrapped by `Framed`.
    ///
    /// # Note
    ///
    /// Care should be taken to not tamper with the underlying stream of data
    /// coming in as it may corrupt the stream of frames otherwise being worked
    /// with.
    pub fn get_ref(&self) -> &UdpSocket {
        &self.inner.inner
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `Framed`.
    ///
    /// # Note
    ///
    /// Care should be taken to not tamper with the underlying stream of data
    /// coming in as it may corrupt the stream of frames otherwise being worked
    /// with.
    pub fn get_mut(&mut self) -> &mut UdpSocket {
        &mut self.inner.inner
    }

    /// Returns a reference to the underlying codec wrapped by
    /// `Framed`.
    ///
    /// Note that care should be taken to not tamper with the underlying codec
    /// as it may corrupt the stream of frames otherwise being worked with.
    pub fn codec(&self) -> &C {
        &self.inner.codec
    }

    /// Returns a mutable reference to the underlying codec wrapped by
    /// `UdpFramed`.
    ///
    /// Note that care should be taken to not tamper with the underlying codec
    /// as it may corrupt the stream of frames otherwise being worked with.
    pub fn codec_mut(&mut self) -> &mut C {
        &mut self.inner.codec
    }

    /// Returns a reference to the read buffer.
    pub fn read_buffer(&self) -> &BytesMut {
        &self.inner.state.read.buffer
    }

    /// Returns a mutable reference to the read buffer.
    pub fn read_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.inner.state.read.buffer
    }

    /// Returns a reference to the write buffer.
    pub fn write_buffer(&self) -> &BytesMut {
        &self.inner.state.write.buffer
    }

    /// Returns a mutable reference to the write buffer.
    pub fn write_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.inner.state.write.buffer
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    pub fn into_inner(self) -> UdpSocket {
        self.inner.inner
    }
}

impl<C> fmt::Debug for UdpFramed<C>
where
    C: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpFramed")
            .field("io", self.get_ref())
            .field("codec", self.codec())
            .field("current_addr", &self.inner.current_addr)
            .field("flushed", &self.inner.flushed)
            .field("out_addr", &self.inner.out_addr)
            .field("is_readable", &self.inner.state.read.is_readable)
            .field("eof", &self.inner.state.read.eof)
            .field("read_buffer", &self.read_buffer())
            .field("write_buffer", &self.write_buffer())
            .finish()
    }
}
