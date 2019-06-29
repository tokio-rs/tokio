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
pub struct SendTo<'a, 'b> {
    socket: &'a UdpSocket,
    buf: &'b [u8],
    target: &'b SocketAddr,
}

impl<'a, 'b> SendTo<'a, 'b> {
    pub(super) fn new(socket: &'a UdpSocket, buf: &'b [u8], target: &'b SocketAddr) -> Self {
        Self {
            socket,
            buf,
            target,
        }
    }
}

impl<'a, 'b> Future for SendTo<'a, 'b> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let SendTo {
            socket,
            buf,
            target,
        } = self.get_mut();
        socket.poll_send_to_priv(cx, buf, target)
    }
}
