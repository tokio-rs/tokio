use std::io;

use futures::{Async, AsyncSink, Poll, Sink, StartSend, Stream};

use super::UnixDatagram;

use bytes::{BufMut, BytesMut};
use tokio_codec::{Decoder, Encoder};

/// A unified `Stream` and `Sink` interface to an underlying connected `UnixDatagram`,
/// using the `Encoder` and `Decoder` traits to encode and decode frames.
///
/// This version differs from `UnixDatagramFramed`. It communicates with `connected`
/// peer, and doesn't deal with the remote addr in both `Stream` and `Sink` interfaces.
/// See [`connect`] for details)
///
/// This is especially useful with [unnamed pair] of `UnixDatagram`
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
///
/// [`connect`]: struct.UnixDatagram.html#method.connect
/// [unnamed pair]: struct.UnixDatagram.html#method.pair
#[must_use = "sinks do nothing unless polled"]
#[derive(Debug)]
pub struct UnixDatagramConnectedFramed<C> {
    socket: UnixDatagram,
    codec: C,
    rd: BytesMut,
    wr: BytesMut,
    flushed: bool,
}

impl<C: Decoder> Stream for UnixDatagramConnectedFramed<C> {
    type Item = C::Item;
    type Error = C::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.rd.reserve(INITIAL_RD_CAPACITY);

        let n = unsafe {
            let n = try_ready!(self.socket.poll_recv(self.rd.bytes_mut()));
            self.rd.advance_mut(n);
            n
        };
        trace!("received {} bytes, decoding", n);
        let frame_res = self.codec.decode(&mut self.rd);
        self.rd.clear();
        let frame = frame_res?;
        trace!("frame decoded from buffer");
        Ok(Async::Ready(frame))
    }
}

impl<C: Encoder> Sink for UnixDatagramConnectedFramed<C> {
    type SinkItem = C::Item;
    type SinkError = C::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        trace!("sending frame");

        if !self.flushed {
            match try!(self.poll_complete()) {
                Async::Ready(()) => {}
                Async::NotReady => return Ok(AsyncSink::NotReady(item)),
            }
        }

        self.codec.encode(item, &mut self.wr)?;
        self.flushed = false;
        trace!("frame encoded; length={}", self.wr.len());

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), C::Error> {
        if self.flushed {
            return Ok(Async::Ready(()));
        }

        let n = {
            trace!("flushing frame; length={}", self.wr.len());
            try_ready!(self.socket.poll_send(&self.wr))
        };

        trace!("written {}", n);

        let wrote_all = n == self.wr.len();
        self.wr.clear();
        self.flushed = true;

        if wrote_all {
            Ok(Async::Ready(()))
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to write entire datagram to socket",
            )
            .into())
        }
    }

    fn close(&mut self) -> Poll<(), C::Error> {
        self.poll_complete()
    }
}

const INITIAL_RD_CAPACITY: usize = 64 * 1024;
const INITIAL_WR_CAPACITY: usize = 8 * 1024;

impl<C> UnixDatagramConnectedFramed<C> {
    /// Create a new `UnixDatagramFramed` backed by the given socket and codec.
    ///
    /// See struct level documentation for more details.
    pub fn new(socket: UnixDatagram, codec: C) -> UnixDatagramConnectedFramed<C> {
        UnixDatagramConnectedFramed {
            socket: socket,
            codec: codec,
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
