use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_udp::UdpSocket;

/// A future that sends a datagram to a given address.
///
/// This `struct` is created by [`send_to_async`](super::UdpSocketAsyncExt::send_to_async).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SendToAsync<'a> {
    socket: &'a mut UdpSocket,
    buf: &'a [u8],
    addr: &'a SocketAddr,
}

impl<'a> SendToAsync<'a> {
    pub(super) fn new(socket: &'a mut UdpSocket, buf: &'a [u8], addr: &'a SocketAddr) -> Self {
        Self { socket, buf, addr }
    }
}

impl<'a> Future for SendToAsync<'a> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;
        let _self = self.get_mut();
        convert_poll(_self.socket.poll_send_to(_self.buf, _self.addr))
    }
}
