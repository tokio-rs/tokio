use super::socket::UdpSocket;

use std::io;
use std::net::SocketAddr;

use futures::{Async, Future, Poll};

/// A future used to write the entire contents of some data to a UDP socket.
///
/// This is created by the `UdpSocket::send_dgram` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SendDgram<T> {
    /// None means future was completed
    state: Option<SendDgramInner<T>>
}

/// A struct is used to represent the full info of SendDgram.
#[derive(Debug)]
struct SendDgramInner<T> {
    /// Tx socket
    socket: UdpSocket,
    /// The whole buffer will be sent
    buffer: T,
    /// Destination addr
    addr: SocketAddr,
}

impl<T> SendDgram<T> {
    /// Create a new future to send UDP Datagram
    pub(crate) fn new(socket: UdpSocket, buffer: T, addr: SocketAddr) -> SendDgram<T> {
        let inner = SendDgramInner { socket: socket, buffer: buffer, addr: addr };
        SendDgram { state: Some(inner) }
    }
}

fn incomplete_write(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, reason)
}

impl<T> Future for SendDgram<T>
    where T: AsRef<[u8]>,
{
    type Item = (UdpSocket, T);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(UdpSocket, T), io::Error> {
        {
            let ref mut inner =
                self.state.as_mut().expect("SendDgram polled after completion");
            let n = try_ready!(inner.socket.poll_send_to(inner.buffer.as_ref(), &inner.addr));
            if n != inner.buffer.as_ref().len() {
                return Err(incomplete_write("failed to send entire message \
                                             in datagram"))
            }
        }

        let inner = self.state.take().unwrap();
        Ok(Async::Ready((inner.socket, inner.buffer)))
    }
}
