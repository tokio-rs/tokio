use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_udp::UdpSocket;

/// A future that sends a datagram to the connected address.
///
/// This `struct` is created by [`send_async`](super::UdpSocketAsyncExt::send_async).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SendAsync<'a> {
    socket: &'a mut UdpSocket,
    buf: &'a [u8],
}

impl<'a> SendAsync<'a> {
    pub(super) fn new(socket: &'a mut UdpSocket, buf: &'a [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'a> Future for SendAsync<'a> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;
        let _self = self.get_mut();
        convert_poll(_self.socket.poll_send(_self.buf))
    }
}
