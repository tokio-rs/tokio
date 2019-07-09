use crate::UnixDatagram;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that sends a datagram to a given address.
///
/// This `struct` is created by [`send_to`](crate::UnixDatagram::send_to).
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SendTo<'a, 'b, P> {
    socket: &'a UnixDatagram,
    buf: &'b [u8],
    target: P,
}

impl<'a, 'b, P> SendTo<'a, 'b, P> {
    pub(crate) fn new(socket: &'a UnixDatagram, buf: &'b [u8], target: P) -> Self {
        Self {
            socket,
            buf,
            target,
        }
    }
}

impl<'a, 'b, P> Future for SendTo<'a, 'b, P>
where
    P: AsRef<Path> + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let SendTo {
            socket,
            buf,
            target,
        } = self.get_mut();
        socket.poll_send_to_priv(cx, buf, target.as_ref())
    }
}
