use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_udp::UdpSocket;

/// A future that receives a datagram.
///
/// This `struct` is created by [`recv_from_async`](super::UdpSocketAsyncExt::recv_from_async).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvFromAsync<'a> {
    socket: &'a mut UdpSocket,
    buf: &'a mut [u8],
}

impl<'a> RecvFromAsync<'a> {
    pub(super) fn new(socket: &'a mut UdpSocket, buf: &'a mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'a> Future for RecvFromAsync<'a> {
    type Output = io::Result<(usize, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;
        let _self = self.get_mut();
        convert_poll(_self.socket.poll_recv_from(_self.buf))
    }
}
