use super::socket::UdpSocket;

use std::io;
use std::net::SocketAddr;

use futures::{Async, Future, Poll};

/// A future used to receive a datagram from a UDP socket.
///
/// This is created by the `UdpSocket::recv_dgram` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvDgram<T> {
    /// None means future was completed
    state: Option<RecvDgramInner<T>>
}

/// A struct is used to represent the full info of RecvDgram.
#[derive(Debug)]
struct RecvDgramInner<T> {
    /// Rx socket
    socket: UdpSocket,
    /// The received data will be put in the buffer
    buffer: T
}

impl<T> RecvDgram<T> {
    /// Create a new future to receive UDP Datagram
    pub(crate) fn new(socket: UdpSocket, buffer: T) -> RecvDgram<T> {
        let inner = RecvDgramInner { socket: socket, buffer: buffer };
        RecvDgram { state: Some(inner) }
    }
}

impl<T> Future for RecvDgram<T>
    where T: AsMut<[u8]>,
{
    type Item = (UdpSocket, T, usize, SocketAddr);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, io::Error> {
        let (n, addr) = {
            let ref mut inner =
                self.state.as_mut().expect("RecvDgram polled after completion");

            try_ready!(inner.socket.poll_recv_from(inner.buffer.as_mut()))
        };

        let inner = self.state.take().unwrap();
        Ok(Async::Ready((inner.socket, inner.buffer, n, addr)))
    }
}
