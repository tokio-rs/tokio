//! UDP framing

use std::{
    borrow::Borrow,
    io,
    net::SocketAddr,
    task::{Context, Poll},
};
use tokio::{io::ReadBuf, net::UdpSocket};

mod frame;
pub use frame::UdpFramed;

/// Types that support receiving datagrams.
///
/// This trait is implemented for any types that implement [`Borrow`]<[`UdpSocket`]>.
pub trait PollRecvFrom {
    /// Attempts to receive a single datagram on the socket.
    ///
    /// See [`UdpSocket::poll_recv_from()`] for more information.
    fn poll_recv_from(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<SocketAddr>>;
}

impl<T: Borrow<UdpSocket>> PollRecvFrom for T {
    fn poll_recv_from(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<SocketAddr>> {
        (*self).borrow().poll_recv_from(cx, buf)
    }
}

/// Types that support sending datagrams to [`SocketAddr`]s.
///
/// This trait is implemented for any types that implement [`Borrow`]<[`UdpSocket`]>.
pub trait PollSendTo {
    /// Attempts to send data on the socket to a given address.
    ///
    /// See [`UdpSocket::poll_send_to()`] for more information.
    fn poll_send_to(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: SocketAddr,
    ) -> Poll<io::Result<usize>>;
}

impl<T: Borrow<UdpSocket>> PollSendTo for T {
    fn poll_send_to(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: SocketAddr,
    ) -> Poll<io::Result<usize>> {
        (*self).borrow().poll_send_to(cx, buf, target)
    }
}
