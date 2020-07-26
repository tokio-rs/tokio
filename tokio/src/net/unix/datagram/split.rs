//! `UnixDatagram` split support.
//!
//! A `UnixDatagram` can be split into a `RecvHalf` and a `SendHalf` with the
//! `UnixDatagram::split` method.

use crate::future::poll_fn;
use crate::net::UnixDatagram;

use std::io;
use std::os::unix::net::SocketAddr;
use std::path::Path;

/// Borrowed receive half of a [`UnixDatagram`], created by [`split`].
///
/// [`UnixDatagram`]: UnixDatagram
/// [`split`]: crate::net::UnixDatagram::split()
#[derive(Debug)]
pub struct RecvHalf<'a>(&'a UnixDatagram);

/// Borrowed send half of a [`UnixDatagram`], created by [`split`].
///
/// [`UnixDatagram`]: UnixDatagram
/// [`split`]: crate::net::UnixDatagram::split()
#[derive(Debug)]
pub struct SendHalf<'a>(&'a UnixDatagram);

pub(crate) fn split(stream: &mut UnixDatagram) -> (RecvHalf<'_>, SendHalf<'_>) {
    (RecvHalf(&*stream), SendHalf(&*stream))
}

impl RecvHalf<'_> {
    /// Receives data from the socket.
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        poll_fn(|cx| self.0.poll_recv_from_priv(cx, buf)).await
    }

    /// Receives data from the socket.
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.0.poll_recv_priv(cx, buf)).await
    }
}

impl SendHalf<'_> {
    /// Sends data on the socket to the specified address.
    pub async fn send_to<P>(&mut self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path> + Unpin,
    {
        poll_fn(|cx| self.0.poll_send_to_priv(cx, buf, target.as_ref())).await
    }

    /// Sends data on the socket to the socket's peer.
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        poll_fn(|cx| self.0.poll_send_priv(cx, buf)).await
    }
}

impl AsRef<UnixDatagram> for RecvHalf<'_> {
    fn as_ref(&self) -> &UnixDatagram {
        self.0
    }
}

impl AsRef<UnixDatagram> for SendHalf<'_> {
    fn as_ref(&self) -> &UnixDatagram {
        self.0
    }
}
