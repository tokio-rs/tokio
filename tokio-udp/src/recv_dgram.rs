use super::socket::UdpSocket;
use futures::{try_ready, Async, Future, Poll};
use std::io;
use std::net::SocketAddr;

/// A future used to receive a datagram from a UDP socket.
///
/// This is created by the `UdpSocket::recv_dgram` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvDgram<T> {
    /// None means future was completed
    state: Option<RecvDgramInner<T>>,
}

/// A struct is used to represent the full info of RecvDgram.
#[derive(Debug)]
struct RecvDgramInner<T> {
    /// Rx socket
    socket: UdpSocket,
    /// The received data will be put in the buffer
    buffer: T,
}

/// Components of a `RecvDgram` future, returned from `into_parts`.
#[derive(Debug)]
pub struct Parts<T> {
    /// The socket
    pub socket: UdpSocket,
    /// The buffer
    pub buffer: T,

    _priv: (),
}

impl<T> RecvDgram<T> {
    /// Create a new future to receive UDP Datagram
    pub(crate) fn new(socket: UdpSocket, buffer: T) -> RecvDgram<T> {
        let inner = RecvDgramInner {
            socket: socket,
            buffer: buffer,
        };
        RecvDgram { state: Some(inner) }
    }

    /// Consume the `RecvDgram`, returning the socket and buffer.
    ///
    /// # Panics
    ///
    /// If called after the future has completed.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_udp::UdpSocket;
    ///
    /// let socket = UdpSocket::bind(&([127, 0, 0, 1], 0).into()).unwrap();
    /// let mut buffer = vec![0; 4096];
    ///
    /// let future = socket.recv_dgram(buffer);
    ///
    /// // ... polling `future` ... giving up (e.g. after timeout)
    ///
    /// let parts = future.into_parts();
    ///
    /// let socket = parts.socket; // extract the socket
    /// let buffer = parts.buffer; // extract the buffer
    /// ```
    pub fn into_parts(mut self) -> Parts<T> {
        let state = self
            .state
            .take()
            .expect("into_parts called after completion");

        Parts {
            socket: state.socket,
            buffer: state.buffer,
            _priv: (),
        }
    }
}

impl<T> Future for RecvDgram<T>
where
    T: AsMut<[u8]>,
{
    type Item = (UdpSocket, T, usize, SocketAddr);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, io::Error> {
        let (n, addr) = {
            let ref mut inner = self
                .state
                .as_mut()
                .expect("RecvDgram polled after completion");

            try_ready!(inner.socket.poll_recv_from(inner.buffer.as_mut()))
        };

        let inner = self.state.take().unwrap();
        Ok(Async::Ready((inner.socket, inner.buffer, n, addr)))
    }
}
