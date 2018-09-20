use std::io;
use std::os::unix::net::SocketAddr;
use std::path::Path;

use futures::{Async, Poll, Stream, Sink, StartSend, AsyncSink};

use super::UnixDatagram;

use tokio_codec::{Decoder, Encoder};
use bytes::{BytesMut, BufMut};

/// A unified `Stream` and `Sink` interface to an underlying `UnixDatagram`, using
/// the `Encoder` and `Decoder` traits to encode and decode frames.
///
/// Unix datagram sockets work with datagrams, but higher-level code may wants to
/// batch these into meaningful chunks, called "frames". This method layers
/// framing on top of this socket by using the `Encoder` and `Decoder` traits to
/// handle encoding and decoding of messages frames. Note that the incoming and
/// outgoing frame types may be distinct.
///
/// This function returns a *single* object that is both `Stream` and `Sink`;
/// grouping this into a single object is often useful for layering things which
/// require both read and write access to the underlying object.
///
/// If you want to work more directly with the streams and sink, consider
/// calling `split` on the `UnixDatagramFramed` returned by this method, which will break
/// them into separate objects, allowing them to interact more easily.
#[must_use = "sinks do nothing unless polled"]
#[derive(Debug)]
pub struct UnixDatagramFramed<A, C> {
    socket: UnixDatagram,
    codec: C,
    rd: BytesMut,
    wr: BytesMut,
    out_addr: Option<A>,
    flushed: bool,
}

impl<A, C: Decoder> Stream for UnixDatagramFramed<A, C> {
    type Item = (C::Item, SocketAddr);
    type Error = C::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.rd.reserve(INITIAL_RD_CAPACITY);

        let (n, addr) = unsafe {
            let (n, addr) = try_ready!(self.socket.poll_recv_from(self.rd.bytes_mut()));
            self.rd.advance_mut(n);
            (n, addr)
        };
        trace!("received {} bytes, decoding", n);
        let frame_res = self.codec.decode(&mut self.rd);
        self.rd.clear();
        let frame = frame_res?;
        let result = frame.map(|frame| (frame, addr));
        trace!("frame decoded from buffer");
        Ok(Async::Ready(result))
    }
}

impl<A: AsRef<Path>, C: Encoder> Sink for UnixDatagramFramed<A, C> {
    type SinkItem = (C::Item, A);
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
        self.out_addr = Some(out_addr);
        self.flushed = false;
        trace!("frame encoded; length={}", self.wr.len());

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), C::Error> {
        if self.flushed {
            return Ok(Async::Ready(()))
        }

        let n = {
            let out_path = match self.out_addr {
                Some(ref out_path) => out_path.as_ref(),
                None => return Err(io::Error::new(io::ErrorKind::Other,
                                                      "internal error: addr not available while data not flushed").into()),
            };

            trace!("flushing frame; length={}", self.wr.len());
            try_ready!(self.socket.poll_send_to(&self.wr, out_path))
        };

        trace!("written {}", n);

        let wrote_all = n == self.wr.len();
        self.wr.clear();
        self.flushed = true;

        if wrote_all {
            self.out_addr = None;
            Ok(Async::Ready(()))
        } else {
            Err(io::Error::new(io::ErrorKind::Other,
                               "failed to write entire datagram to socket").into())
        }
    }

    fn close(&mut self) -> Poll<(), C::Error> {
        self.poll_complete()
    }
}

const INITIAL_RD_CAPACITY: usize = 64 * 1024;
const INITIAL_WR_CAPACITY: usize = 8 * 1024;

impl<A, C> UnixDatagramFramed<A, C> {
    /// Create a new `UnixDatagramFramed` backed by the given socket and codec.
    ///
    /// See struct level documentation for more details.
    pub fn new(socket: UnixDatagram, codec: C) -> UnixDatagramFramed<A, C> {
        UnixDatagramFramed {
            socket: socket,
            codec: codec,
            out_addr: None,
            rd: BytesMut::with_capacity(INITIAL_RD_CAPACITY),
            wr: BytesMut::with_capacity(INITIAL_WR_CAPACITY),
            flushed: true,
        }
    }

    /// Returns a reference to the underlying I/O stream wrapped by `Framed`.
    ///
    /// # Note
    ///
    /// Care should be taken to not tamper with the underlying stream of data
    /// coming in as it may corrupt the stream of frames otherwise being worked
    /// with.
    pub fn get_ref(&self) -> &UnixDatagram {
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
    pub fn get_mut(&mut self) -> &mut UnixDatagram {
        &mut self.socket
    }
}
