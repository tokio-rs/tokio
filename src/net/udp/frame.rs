use std::io;
use std::net::{SocketAddr, Ipv4Addr, SocketAddrV4};

use futures::{Async, Poll, Stream, Sink, StartSend, AsyncSink};

use net::UdpSocket;

/// Encoding of datagrams into frames via buffers.
///
/// This trait is used when constructing an instance of `UdpFramed` and provides
/// the `In` and `Out` types which are decoded and encoded from the socket,
/// respectively.
///
/// Because UDP is a connectionless protocol, the `decode` method receives the
/// address where data came from and the `encode` method is also responsible for
/// determining the remote host to which the datagram should be sent
///
/// The trait itself is implemented on a type that can track state for decoding
/// or encoding, which is particularly useful for streaming parsers. In many
/// cases, though, this type will simply be a unit struct (e.g. `struct
/// MyCodec`).
pub trait UdpCodec {
    /// The type of decoded frames.
    type In;

    /// The type of frames to be encoded.
    type Out;

    /// The type of unrecoverable frame encoding/decoding errors.
    ///
    /// If an individual message is ill-formed but can be ignored without
    /// interfering with the processing of future messages, it may be more
    /// useful to report the failure as an `Item`.
    ///
    /// Note that implementors of this trait can simply indicate `type Error =
    /// io::Error` to use I/O errors as this type.
    type Error: From<io::Error>;

    /// Attempts to decode a frame from the provided buffer of bytes.
    ///
    /// This method is called by `UdpFramed` on a single datagram which has been
    /// read from a socket. The `buf` argument contains the data that was
    /// received from the remote address, and `src` is the address the data came
    /// from. Note that typically this method should require the entire contents
    /// of `buf` to be valid or otherwise return an error with trailing data.
    ///
    /// Finally, if the bytes in the buffer are malformed then an error is
    /// returned indicating why. This informs `Framed` that the stream is now
    /// corrupt and should be terminated.
    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> Result<Self::In, Self::Error>;

    /// Encodes a frame into the buffer provided.
    ///
    /// This method will encode `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `Framed` instance and
    /// will be written out when possible.
    ///
    /// The encode method also determines the destination to which the buffer
    /// should be directed, which will be returned as a `SocketAddr`.
    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> Result<SocketAddr, Self::Error>;
}

/// A unified `Stream` and `Sink` interface to an underlying `UdpSocket`, using
/// the `UdpCodec` trait to encode and decode frames.
///
/// You can acquire a `UdpFramed` instance by using the `UdpSocket::framed`
/// adapter.
#[must_use = "sinks do nothing unless polled"]
#[derive(Debug)]
pub struct UdpFramed<C> {
    socket: UdpSocket,
    codec: C,
    rd: Vec<u8>,
    wr: Vec<u8>,
    out_addr: SocketAddr,
    flushed: bool,
}

impl<C: UdpCodec> Stream for UdpFramed<C> {
    type Item = C::In;
    type Error = C::Error;

    fn poll(&mut self) -> Poll<Option<C::In>, C::Error> {
        let (n, addr) = try_nb!(self.socket.recv_from(&mut self.rd));
        trace!("received {} bytes, decoding", n);
        let frame = self.codec.decode(&addr, &self.rd[..n])?;
        trace!("frame decoded from buffer");
        Ok(Async::Ready(Some(frame)))
    }
}

impl<C: UdpCodec> Sink for UdpFramed<C> {
    type SinkItem = C::Out;
    type SinkError = C::Error;

    fn start_send(&mut self, item: C::Out) -> StartSend<C::Out, C::Error> {
        trace!("sending frame");

        if !self.flushed {
            match try!(self.poll_complete()) {
                Async::Ready(()) => {},
                Async::NotReady => return Ok(AsyncSink::NotReady(item)),
            }
        }

        self.out_addr = self.codec.encode(item, &mut self.wr)?;
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

pub fn new<C: UdpCodec>(socket: UdpSocket, codec: C) -> UdpFramed<C> {
    UdpFramed {
        socket: socket,
        codec: codec,
        out_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
        rd: vec![0; 64 * 1024],
        wr: Vec::with_capacity(8 * 1024),
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
