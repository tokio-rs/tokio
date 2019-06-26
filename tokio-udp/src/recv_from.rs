use super::UdpSocket;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that receives a datagram.
///
/// This `struct` is created by [`recv_from`](super::UdpSocket::recv_from).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvFrom<'socket, 'buf> {
    socket: &'socket mut UdpSocket,
    buf: &'buf mut [u8],
}

impl<'socket, 'buf> RecvFrom<'socket, 'buf> {
    pub(super) fn new(socket: &'socket mut UdpSocket, buf: &'buf mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'socket, 'buf> Future for RecvFrom<'socket, 'buf> {
    type Output = io::Result<(usize, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RecvFrom { socket, buf } = self.get_mut();
        Pin::new(&mut **socket).poll_recv_from(cx, buf)
    }
}
