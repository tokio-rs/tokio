//! `UdpSocket` split support.
//!
//! A `UdpSocket` can be split into a read half and a write half with
//! `UdpSocket::split`.
//!
//! This is a special `split` that just borrows `UdpSocket`, meaning
//! it's not safe to move into tasks. If that functionality is desired,
//! use the `Owned` variants.

use crate::net::{ToSocketAddrs, UdpSocket};

use std::{io, net::SocketAddr};

/// Borrowed read half of a [`UdpSocket`], created by [`split`].
///
///
/// [`UdpSocket`]: UdpSocket
/// [`split`]: UdpSocket::split()
#[derive(Debug)]
pub struct RecvHalf<'a>(&'a UdpSocket);

/// Borrowed write half of a [`UdpSocket`], created by [`split`].
///
/// [`UdpSocket`]: UdpSocket
/// [`split`]: UdpSocket::split()
#[derive(Debug)]
pub struct SendHalf<'a>(&'a UdpSocket);

pub(crate) fn split(stream: &UdpSocket) -> (RecvHalf<'_>, SendHalf<'_>) {
    (RecvHalf(stream), SendHalf(stream))
}

impl<'a> RecvHalf<'a> {
    /// Attempts to receive a single datagram message on the socket from the remote
    /// address to which it is `connect`ed.
    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf).await
    }

    /// Returns a future that receives a single datagram on the socket. On success,
    /// the future resolves to the number of bytes read and the origin.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size
    /// to hold the message bytes. If a message is too long to fit in the supplied
    /// buffer, excess bytes may be discarded.
    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.0.recv_from(buf).await
    }
}

impl<'a> SendHalf<'a> {
    /// Returns a future that sends data on the socket to the remote address to which it is connected.
    /// On success, the future will resolve to the number of bytes written.
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.send(buf).await
    }

    /// Returns a future that sends data on the socket to the given address.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The future will resolve to an error if the IP version of the socket does
    /// not match that of `target`.
    pub async fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addr: A) -> io::Result<usize> {
        self.0.send_to(buf, addr).await
    }
}

impl AsRef<UdpSocket> for RecvHalf<'_> {
    fn as_ref(&self) -> &UdpSocket {
        self.0
    }
}

impl AsRef<UdpSocket> for SendHalf<'_> {
    fn as_ref(&self) -> &UdpSocket {
        self.0
    }
}
