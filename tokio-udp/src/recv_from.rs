use super::UdpSocket;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that receives a datagram.
///
/// This `struct` is created by [`recv_from`](super::UdpSocket::recv_from).
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct RecvFrom<'a, 'b> {
    socket: &'a UdpSocket,
    buf: &'b mut [u8],
}

impl<'a, 'b> RecvFrom<'a, 'b> {
    pub(super) fn new(socket: &'a UdpSocket, buf: &'b mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl Future for RecvFrom<'_, '_> {
    type Output = io::Result<(usize, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RecvFrom { socket, buf } = self.get_mut();
        socket.poll_recv_from_priv(cx, buf)
    }
}
