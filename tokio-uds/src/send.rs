use crate::UnixDatagram;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that sends a datagram to the connected address.
///
/// This `struct` is created by [`send`](crate::UnixDatagram::send).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Send<'a, 'b> {
    socket: &'a UnixDatagram,
    buf: &'b [u8],
}

impl<'a, 'b> Send<'a, 'b> {
    pub(crate) fn new(socket: &'a UnixDatagram, buf: &'b [u8]) -> Self {
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
