use crate::codec::{Encoder, WriteFrame};
use crate::udp::framed_impl::UdpFramedImpl;

use pin_project_lite::pin_project;
use tokio::net::UdpSocket;

use bytes::BytesMut;
use futures_sink::Sink;
use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    pin::Pin,
    task::{Context, Poll},
};

pin_project! {
    /// A [`Sink`] of frames encoded for udp.
    ///
    /// [`Sink`]: futures_sink::Sink
    #[cfg_attr(docsrs, doc(all(feature = "codec", feature = "udp")))]
    pub struct UdpFramedWrite<C> {
        #[pin]
        inner: UdpFramedImpl<C, WriteFrame>,
    }
}

impl<C> UdpFramedWrite<C> {
    /// Create a new `UdpFramed` backed by the given socket and codec.
    ///
    /// See struct level documentation for more details.
    pub fn new(socket: UdpSocket, codec: C) -> UdpFramedWrite<C> {
        Self {
            inner: UdpFramedImpl {
                codec,
                state: WriteFrame {
                    buffer: BytesMut::with_capacity(crate::udp::framed_impl::INITIAL_WR_CAPACITY),
                },
                inner: socket,
                current_addr: None,
                out_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
                flushed: false,
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
    /// `UdpFramed`.
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
    pub fn encoder(&self) -> &C {
        &self.inner.codec
    }

    /// Returns a mutable reference to the underlying codec wrapped by
    /// `UdpFramed`.
    ///
    /// Note that care should be taken to not tamper with the underlying codec
    /// as it may corrupt the stream of frames otherwise being worked with.
    pub fn encoder_mut(&mut self) -> &mut C {
        &mut self.inner.codec
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    pub fn into_inner(self) -> UdpSocket {
        self.inner.inner
    }
}

// This impl just defers to the underlying FramedImpl
impl<I, U> Sink<(I, SocketAddr)> for UdpFramedWrite<U>
where
    U: Encoder<I>,
    U::Error: From<io::Error>,
{
    type Error = U::Error;

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

impl<C> fmt::Debug for UdpFramedWrite<C>
where
    C: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpFramedWrite")
            .field("io", self.get_ref())
            .field("codec", self.encoder())
            .field("buffer", &self.inner.state.buffer)
            .finish()
    }
}
