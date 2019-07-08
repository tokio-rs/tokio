use super::UdpSocket;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that sends a datagram to the connected address.
///
/// This `struct` is created by [`send`](super::UdpSocket::send).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Send<'a, 'b> {
    socket: &'a UdpSocket,
    buf: &'b [u8],
}

impl<'a, 'b> Send<'a, 'b> {
    pub(super) fn new(socket: &'a UdpSocket, buf: &'b [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'a, 'b> Future for Send<'a, 'b> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Send { socket, buf } = self.get_mut();
        socket.poll_send_priv(cx, buf)
    }
}
