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
pub struct Send<'socket, 'buf> {
    socket: &'socket mut UdpSocket,
    buf: &'buf [u8],
}

impl<'socket, 'buf> Send<'socket, 'buf> {
    pub(super) fn new(socket: &'socket mut UdpSocket, buf: &'buf [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'socket, 'buf> Future for Send<'socket, 'buf> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Send { socket, buf } = self.get_mut();
        Pin::new(&mut **socket).poll_send(cx, buf)
    }
}
