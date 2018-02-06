use std::io;
use std::net::{SocketAddr, Ipv4Addr, SocketAddrV4};

use futures::{Async, Poll, Stream, Sink, StartSend, AsyncSink};

use net::UdpSocket;

use tokio_io::codec::{Decoder, Encoder};
use bytes::{BytesMut, BufMut};

/// A unified `Stream` and `Sink` interface to an underlying `UdpSocket`, using
/// the `Encoder` and `Decoder` traits to encode and decode frames.
///
/// You can acquire a `UdpFramed` instance by using the `UdpSocket::framed`
/// adapter.
#[must_use = "sinks do nothing unless polled"]
#[derive(Debug)]
pub struct UdpFramed<C> {
    socket: UdpSocket,
    codec: C,
    rd: BytesMut,
    wr: BytesMut,
    out_addr: SocketAddr,
    flushed: bool,
}

impl<C: Decoder> Stream for UdpFramed<C> {
    type Item = (C::Item, SocketAddr);
    type Error = C::Error;

    fn poll(&mut self) -> Poll<Option<(Self::Item)>, Self::Error> {
        self.rd.reserve(INITIAL_RD_CAPACITY);

        let (n, addr) = unsafe {
            // Read into the buffer without having to initialize the memory.
            let (n, addr) = try_nb!(self.socket.recv_from(self.rd.bytes_mut()));
            self.rd.advance_mut(n);
            (n, addr)
        };
        trace!("received {} bytes, decoding", n);
        let frame_res = self.codec.decode(&mut self.rd);
        self.rd.clear();
        let frame = frame_res?;
        let result = frame.map(|frame| (frame, addr)); // frame -> (frame, addr)
        trace!("frame decoded from buffer");
        Ok(Async::Ready(result))
    }
}

impl<C: Encoder> Sink for UdpFramed<C> {
    type SinkItem = (C::Item, SocketAddr);
    type SinkError = C::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        trace!("sending frame");

        if !self.flushed {
            match try!(self.poll_complete()) {
                Async::Ready(()) => {},
                Async::NotReady => return Ok(AsyncSink::NotReady(item)),
            }
        }

        let (frame, out_addr) = item;
        self.codec.encode(frame, &mut self.wr)?;
        self.out_addr = out_addr;
        self.flushed = false;
        trace!("frame encoded; length={}", self.wr.len());

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), C::Error> {
        if self.flushed {
            return Ok(Async::Ready(()))
        }

        trace!("flushing frame; length={}", self.wr.len());
        let n = try_nb!(self.socket.send_to(&self.wr, &self.out_addr));
        trace!("written {}", n);

        let wrote_all = n == self.wr.len();
        self.wr.clear();
        self.flushed = true;

        if wrote_all {
            Ok(Async::Ready(()))
        } else {
            Err(io::Error::new(io::ErrorKind::Other,
                               "failed to write entire datagram to socket").into())
        }
    }

    fn close(&mut self) -> Poll<(), C::Error> {
        try_ready!(self.poll_complete());
        Ok(().into())
    }
}

const INITIAL_RD_CAPACITY: usize = 64 * 1024;
const INITIAL_WR_CAPACITY: usize = 8 * 1024;

pub fn new<C: Encoder + Decoder>(socket: UdpSocket, codec: C) -> UdpFramed<C> {
    UdpFramed {
        socket: socket,
        codec: codec,
        out_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
        rd: BytesMut::with_capacity(INITIAL_RD_CAPACITY),
        wr: BytesMut::with_capacity(INITIAL_WR_CAPACITY),
        flushed: true,
    }
}

impl<C> UdpFramed<C> {
    /// Returns a reference to the underlying I/O stream wrapped by `Framed`.
    ///
    /// # Note
    ///
    /// Care should be taken to not tamper with the underlying stream of data
    /// coming in as it may corrupt the stream of frames otherwise being worked
    /// with.
    pub fn get_ref(&self) -> &UdpSocket {
        &self.socket
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
        &mut self.socket
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    pub fn into_inner(self) -> UdpSocket {
        self.socket
    }
}
