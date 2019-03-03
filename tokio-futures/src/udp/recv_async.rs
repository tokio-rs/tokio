use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_udp::UdpSocket;

/// A future that receives a datagram from the connected address.
///
/// This `struct` is created by [`recv_async`](super::UdpSocketAsyncExt::recv_async).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvAsync<'a> {
    socket: &'a mut UdpSocket,
    buf: &'a mut [u8],
}

impl<'a> RecvAsync<'a> {
    pub(super) fn new(socket: &'a mut UdpSocket, buf: &'a mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'a> Future for RecvAsync<'a> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;
        let _self = self.get_mut();
        convert_poll(_self.socket.poll_recv(_self.buf))
    }
}
