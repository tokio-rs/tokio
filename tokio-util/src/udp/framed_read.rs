use crate::codec::{Decoder, ReadFrame};
use crate::udp::framed_impl::UdpFramedImpl;

use pin_project_lite::pin_project;
use tokio::net::UdpSocket;
use tokio_stream::Stream;

use bytes::BytesMut;
use std::{
    fmt,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    pin::Pin,
    task::{Context, Poll},
};

pin_project! {
    #[cfg_attr(docsrs, doc(all(feature = "codec", feature = "udp")))]
    pub struct UdpFramedRead<C> {
        #[pin]
        inner: UdpFramedImpl<C, ReadFrame>,
    }
}

impl<C> UdpFramedRead<C> {
    /// Create a new `UdpFramed` backed by the given socket and codec.
    ///
    /// See struct level documentation for more details.
    pub fn new(socket: UdpSocket, codec: C) -> UdpFramedRead<C> {
        Self {
            inner: UdpFramedImpl {
                codec,
                state: ReadFrame {
                    buffer: BytesMut::with_capacity(crate::udp::framed_impl::INITIAL_RD_CAPACITY),
                    ..ReadFrame::default()
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
        &self.inner.state.buffer
    }

    /// Returns a mutable reference to the read buffer.
    pub fn read_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.inner.state.buffer
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    pub fn into_inner(self) -> UdpSocket {
        self.inner.inner
    }
}

impl<C> Stream for UdpFramedRead<C>
where
    C: Decoder,
{
    type Item = Result<(C::Item, SocketAddr), C::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

impl<C> fmt::Debug for UdpFramedRead<C>
where
    C: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpFramedRead")
            .field("io", self.get_ref())
            .field("codec", self.codec())
            .field("current_addr", &self.inner.current_addr)
            .field("is_readable", &self.inner.state.is_readable)
            .field("eof", &self.inner.state.eof)
            .field("read_buffer", &self.read_buffer())
            .finish()
    }
}
