use crate::{Decoder, Encoder};

use bytes::{BufMut, BytesMut};
use core::task::{Context, Poll};
use futures_core::{ready, Stream};
use futures_sink::Sink;
use log::trace;
use std::io;
use std::pin::Pin;
use tokio_io::AsyncDatagram;

/// A unified `Stream` and `Sink` interface to an underlying datagram socket, using
/// the `Encoder` and `Decoder` traits to encode and decode frames.
///
/// Even for datagram-based sockets, higher-level code usually wants to
/// batch these into meaningful chunks, called "frames". This method layers
/// framing on top of this socket by using the `Encoder` and `Decoder` traits to
/// handle encoding and decoding of messages frames. Note that the incoming and
/// outgoing frame types may be distinct.
///
/// This function returns a *single* object that is both `Stream` and `Sink`;
/// grouping this into a single object is often useful for layering things which
/// require both read and write access to the underlying object.
#[must_use = "sinks do nothing unless polled"]
#[derive(Debug)]
pub struct DatagramFramed<C, S, A> {
    socket: S,
    codec: C,
    rd: BytesMut,
    wr: BytesMut,
    out_addr: Option<A>,
    flushed: bool,
}

impl<C: Decoder + Unpin, S: AsyncDatagram + Unpin> Stream for DatagramFramed<C, S, S::Address> {
    type Item = Result<(C::Item, S::Address), C::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pin = self.get_mut();

        pin.rd.reserve(INITIAL_RD_CAPACITY);

        let (n, addr) = unsafe {
            // Read into the buffer without having to initialize the memory.
            let res = ready!(Pin::new(&mut pin.socket).poll_datagram_recv(cx, pin.rd.bytes_mut()));
            let (n, addr) = res?;
            pin.rd.advance_mut(n);
            (n, addr)
        };
        trace!("received {} bytes, decoding", n);
        let frame_res = pin.codec.decode(&mut pin.rd);
        pin.rd.clear();
        let frame = frame_res?;
        let result = frame.map(|frame| Ok((frame, addr))); // frame -> (frame, addr)

        trace!("frame decoded from buffer");
        Poll::Ready(result)
    }
}

impl<C: Encoder + Unpin, S: AsyncDatagram + Unpin> Sink<(C::Item, S::Address)> for DatagramFramed<C, S, S::Address> {
    type Error = C::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if !self.flushed {
            match self.poll_flush(cx)? {
                Poll::Ready(()) => {}
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: (C::Item, S::Address)) -> Result<(), Self::Error> {
        trace!("sending frame");

        let (frame, out_addr) = item;

        let pin = self.get_mut();

        pin.codec.encode(frame, &mut pin.wr)?;
        pin.out_addr = Some(out_addr);
        pin.flushed = false;
        trace!("frame encoded; length={}", pin.wr.len());

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.flushed {
            return Poll::Ready(Ok(()));
        }

        trace!("flushing frame; length={}", self.wr.len());

        let Self {
            ref mut socket,
            ref mut out_addr,
            ref mut wr,
            ..
        } = *self;

        let n = ready!(socket.poll_datagram_send(cx, wr, out_addr.as_ref().unwrap()))?;
        trace!("written {}", n);

        let wrote_all = n == self.wr.len();
        self.wr.clear();
        self.flushed = true;

        let res = if wrote_all {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to write entire datagram to socket",
            )
                .into())
        };

        Poll::Ready(res)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }
}

const INITIAL_RD_CAPACITY: usize = 64 * 1024;
const INITIAL_WR_CAPACITY: usize = 8 * 1024;

impl<C, S: AsyncDatagram> DatagramFramed<C, S, S::Address> {
    /// Create a new `DatagramFramed` backed by the given socket and codec.
    ///
    /// See struct level documentation for more details.
    pub fn new(socket: S, codec: C) -> Self {
        Self {
            socket,
            codec,
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
    pub fn get_ref(&self) -> &S {
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
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.socket
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    pub fn into_inner(self) -> S {
        self.socket
    }
}
