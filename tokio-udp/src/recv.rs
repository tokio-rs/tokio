use super::UdpSocket;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that receives a datagram from the connected address.
///
/// This `struct` is created by [`recv`](super::UdpSocket::recv).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Recv<'a, 'b> {
    socket: &'a UdpSocket,
    buf: &'b mut [u8],
}

impl<'a, 'b> Recv<'a, 'b> {
    pub(super) fn new(socket: &'a UdpSocket, buf: &'b mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'a, 'b> Future for Recv<'a, 'b> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Recv { socket, buf } = self.get_mut();
        socket.poll_recv_priv(cx, buf)
    }
}
