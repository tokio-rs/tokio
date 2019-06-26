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
pub struct Recv<'socket, 'buf> {
    socket: &'socket mut UdpSocket,
    buf: &'buf mut [u8],
}

impl<'socket, 'buf> Recv<'socket, 'buf> {
    pub(super) fn new(socket: &'socket mut UdpSocket, buf: &'buf mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'socket, 'buf> Future for Recv<'socket, 'buf> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Recv { socket, buf } = self.get_mut();
        Pin::new(&mut **socket).poll_recv(cx, buf)
    }
}
