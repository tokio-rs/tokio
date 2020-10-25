use crate::{codec::Decoder, udp::INITIAL_RD_CAPACITY};

use tokio::{
    io::ReadBuf,
    net::{udp::OwnedRecvHalf, UdpSocket},
    stream::Stream,
};

use bytes::{BufMut, BytesMut};
use futures_core::ready;
use std::{
    mem::MaybeUninit,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Takes a `Decoder` and a `UdpSocket` and allows one to decode frames on a stream.
///
/// Raw UDP sockets work with datagrams, but higher-level code usually wants to
/// batch these into meaningful chunks, called "frames". This method layers
/// framing on top of this socket by using the `Encoder` traits to
/// handle encoding and decoding of messages frames. Note that the incoming and
/// outgoing frame types may be distinct.
///
/// If you want to work more directly with the streams and sink, consider
/// calling `split` on the `UdpFramed` returned by this method, which will break
/// them into separate objects, allowing them to interact more easily.
#[must_use = "sinks do nothing unless polled"]
#[cfg_attr(docsrs, doc(all(feature = "codec", feature = "udp")))]
#[derive(Debug)]
pub struct UdpFramedRecv<C> {
    socket: UdpSocket,
    codec: C,
    rd: BytesMut,
    current_addr: Option<SocketAddr>,
}

impl<C: Decoder + Unpin> Stream for UdpFramedRecv<C> {
    type Item = Result<(C::Item, SocketAddr), C::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pin = self.get_mut();

        pin.rd.reserve(INITIAL_RD_CAPACITY);

        loop {
            // Are there are still bytes left in the read buffer to decode?
            if let Some(frame) = pin.codec.decode_eof(&mut pin.rd)? {
                let current_addr = pin
                    .current_addr
                    .expect("will always be set before this line is called");

                return Poll::Ready(Some(Ok((frame, current_addr))));
            }

            pin.rd.clear();

            // We're out of data. Try and fetch more data to decode
            let addr = unsafe {
                // Convert `&mut [MaybeUnit<u8>]` to `&mut [u8]` because we will be
                // writing to it via `poll_recv_from` and therefore initializing the memory.
                let buf: &mut [u8] =
                    &mut *(pin.rd.bytes_mut() as *mut [MaybeUninit<u8>] as *mut [u8]);
                let mut read = ReadBuf::new(buf);
                let res = ready!(Pin::new(&mut pin.socket).poll_recv_from(cx, &mut read));

                let addr = res?;
                pin.rd.advance_mut(read.filled().len());
                addr
            };

            pin.current_addr = Some(addr);
        }
    }
}

impl<C> UdpFramedRecv<C> {
    pub fn new(socket: UdpSocket, codec: C) -> UdpFramedRecv<C> {
        Self {
            socket,
            codec,
            rd: BytesMut::with_capacity(INITIAL_RD_CAPACITY),
            current_addr: None,
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
