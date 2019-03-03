//! Use [`UdpSocket`](tokio_udp::UdpSocket) with `async`/`await`.

use std::net::SocketAddr;
use tokio_udp::UdpSocket;

mod recv_from_async;
pub use self::recv_from_async::RecvFromAsync;

mod send_to_async;
pub use self::send_to_async::SendToAsync;

mod recv_async;
pub use self::recv_async::RecvAsync;

mod send_async;
pub use self::send_async::SendAsync;

/// Extension trait that adds async/await support for [`UdpSocket`](UdpSocket).
pub trait UdpSocketAsyncExt {
    /// Creates a future that receives a datagram.
    ///
    /// See also [`poll_recv_from`](UdpSocket::poll_recv_from). The future is a simple wrapper around it.
    fn recv_from_async<'a>(&'a mut self, buf: &'a mut [u8]) -> RecvFromAsync<'a>;
    /// Creates a future that sends a datagram to the given address.
    ///
    /// See also [`poll_send_to`](UdpSocket::poll_send_to). The future is a simple wrapper around it.
    fn send_to_async<'a>(&'a mut self, buf: &'a [u8], addr: &'a SocketAddr) -> SendToAsync<'a>;
    /// Creates a future that sends a datagram to the connected address.
    ///
    /// The [`connect`](UdpSocket::connect) method must have been called,
    /// or the returned future will resolve to an error.
    ///
    /// See also [`poll_send`](UdpSocket::poll_send). The future is a simple wrapper around it.
    fn send_async<'a>(&'a mut self, buf: &'a [u8]) -> SendAsync<'a>;
    /// Creates a future that receives a datagram from the connected address.
    ///
    /// The [`connect`](UdpSocket::connect) method must have been called,
    /// or the returned future will resolve to an error.
    ///
    /// See also [`poll_recv`](UdpSocket::poll_recv). The future is a simple wrapper around it.
    fn recv_async<'a>(&'a mut self, buf: &'a mut [u8]) -> RecvAsync<'a>;
}

impl UdpSocketAsyncExt for UdpSocket {
    fn recv_from_async<'a>(&'a mut self, buf: &'a mut [u8]) -> RecvFromAsync<'a> {
        RecvFromAsync::new(self, buf)
    }

    fn send_to_async<'a>(&'a mut self, buf: &'a [u8], addr: &'a SocketAddr) -> SendToAsync<'a> {
        SendToAsync::new(self, buf, addr)
    }

    fn send_async<'a>(&'a mut self, buf: &'a [u8]) -> SendAsync<'a> {
        SendAsync::new(self, buf)
    }

    fn recv_async<'a>(&'a mut self, buf: &'a mut [u8]) -> RecvAsync<'a> {
        RecvAsync::new(self, buf)
    }
}
