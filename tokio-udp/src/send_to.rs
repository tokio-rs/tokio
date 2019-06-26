use super::UdpSocket;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that sends a datagram to a given address.
///
/// This `struct` is created by [`send_to`](super::UdpSocket::send_to).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SendTo<'socket, 'b> {
    socket: &'socket mut UdpSocket,
    buf: &'b [u8],
    target: &'b SocketAddr,
}

impl<'socket, 'b> SendTo<'socket, 'b> {
    pub(super) fn new(
        socket: &'socket mut UdpSocket,
        buf: &'b [u8],
        target: &'b SocketAddr,
    ) -> Self {
        Self {
            socket,
            buf,
            target,
        }
    }
}

impl<'socket, 'b> Future for SendTo<'socket, 'b> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let SendTo {
            socket,
            buf,
            target,
        } = self.get_mut();
        Pin::new(&mut **socket).poll_send_to(cx, buf, target)
    }
}
