use crate::UnixDatagram;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that receives a datagram from the connected address.
///
/// This `struct` is created by [`recv`](crate::UnixDatagram::recv).
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct Recv<'a, 'b> {
    socket: &'a mut UnixDatagram,
    buf: &'b mut [u8],
}

impl<'a, 'b> Recv<'a, 'b> {
    pub(crate) fn new(socket: &'a mut UnixDatagram, buf: &'b mut [u8]) -> Self {
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
