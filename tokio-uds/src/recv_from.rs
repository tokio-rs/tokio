use crate::UnixDatagram;
use std::future::Future;
use std::io;
use std::os::unix::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that receives a datagram.
///
/// This `struct` is created by [`recv_from`](crate::UnixDatagram::recv_from).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvFrom<'a, 'b> {
    socket: &'a UnixDatagram,
    buf: &'b mut [u8],
}

impl<'a, 'b> RecvFrom<'a, 'b> {
    pub(crate) fn new(socket: &'a UnixDatagram, buf: &'b mut [u8]) -> Self {
        Self { socket, buf }
    }
}

impl<'a, 'b> Future for RecvFrom<'a, 'b> {
    type Output = io::Result<(usize, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RecvFrom { socket, buf } = self.get_mut();
        socket.poll_recv_from_priv(cx, buf)
    }
}
