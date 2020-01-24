use crate::net::tcp::{TcpListener, TcpStream};

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream returned by the `TcpListener::incoming` function representing the
/// stream of sockets received from a listener.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Incoming<'a> {
    inner: &'a mut TcpListener,
}

impl Incoming<'_> {
    pub(crate) fn new(listener: &mut TcpListener) -> Incoming<'_> {
        Incoming { inner: listener }
    }

    /// Attempts to poll `TcpStream` by polling inner `TcpListener` to accept
    /// connection.
    ///
    /// If `TcpListener` isn't ready yet, `Poll::Pending` is returned and
    /// current task will be notified by a waker.
    pub fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<TcpStream>> {
        let (socket, _) = ready!(self.inner.poll_accept(cx))?;
        Poll::Ready(Ok(socket))
    }
}

#[cfg(feature = "stream")]
impl crate::stream::Stream for Incoming<'_> {
    type Item = io::Result<TcpStream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(self.inner.poll_accept(cx))?;
        Poll::Ready(Some(Ok(socket)))
    }
}
