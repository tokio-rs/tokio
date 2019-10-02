//! [`UdpSocket`](../struct.UdpSocket.html) split support.
//!
//! The [`split`](../struct.UdpSocket.html#method.split) method splits a
//! `UdpSocket` into a receive half and a send half, which can be used to
//! receive and send datagrams concurrently, even from two different tasks.
//!
//! The halves provide access to the underlying socket, implementing
//! `AsRef<UdpSocket>`. This allows you to call `UdpSocket` methods that takes
//! `&self`, e.g., to get local address, to get and set socket options, to join
//! or leave multicast groups, etc.
//!
//! The halves can be reunited to the original socket with their `reunite`
//! methods.

use super::UdpSocket;

use futures_util::future::poll_fn;
use std::io;
use std::net::SocketAddr;
use crate::ToSocketAddrs;

/// The send half after [`split`](super::UdpSocket::split).
///
/// Use [`send_to`](#method.send_to) or [`send`](#method.send) to send
/// datagrams.
#[derive(Debug)]
pub struct UdpSocketSendHalf<'a>(&'a UdpSocket);

/// The recv half after [`split`](super::UdpSocket::split).
///
/// Use [`recv_from`](#method.recv_from) or [`recv`](#method.recv) to receive
/// datagrams.
#[derive(Debug)]
pub struct UdpSocketRecvHalf<'a>(&'a UdpSocket);

pub(crate) fn split(socket: &mut UdpSocket) -> (UdpSocketRecvHalf<'_>, UdpSocketSendHalf<'_>) {
    (UdpSocketRecvHalf(&*socket), UdpSocketSendHalf(&*socket))
}

impl<'a> UdpSocketRecvHalf<'a> {
    /// Returns a future that receives a single datagram on the socket. On success,
    /// the future resolves to the number of bytes read and the origin.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size
    /// to hold the message bytes. If a message is too long to fit in the supplied
    /// buffer, excess bytes may be discarded.
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        poll_fn(|cx| self.0.poll_recv_from_priv(cx, buf)).await
    }

    /// Returns a future that receives a single datagram message on the socket from
    /// the remote address to which it is connected. On success, the future will resolve
    /// to the number of bytes read.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size to
    /// hold the message bytes. If a message is too long to fit in the supplied buffer,
    /// excess bytes may be discarded.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will fail if the socket is not connected.
    ///
    /// [`connect`]: super::UdpSocket::connect
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.0.poll_recv_priv(cx, buf)).await
    }
}

impl<'a> UdpSocketSendHalf<'a> {
    /// Returns a future that sends data on the socket to the given address.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The future will resolve to an error if the IP version of the socket does
    /// not match that of `target`.
    pub async fn send_to<A: ToSocketAddrs>(&mut self, buf: &[u8], target: A) -> io::Result<usize> {
        let mut addrs = target.to_socket_addrs().await?;

        match addrs.next() {
            Some(target) => poll_fn(|cx| self.0.poll_send_to_priv(cx, buf, &target)).await,
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no addresses to send data to",
            )),
        }
    }

    /// Returns a future that sends data on the socket to the remote address to which it is connected.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will resolve to an error if the socket is not connected.
    ///
    /// [`connect`]: super::UdpSocket::connect
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        poll_fn(|cx| self.0.poll_send_priv(cx, buf)).await
    }
}

impl AsRef<UdpSocket> for UdpSocketSendHalf<'_> {
    fn as_ref(&self) -> &UdpSocket {
        self.0
    }
}

impl AsRef<UdpSocket> for UdpSocketRecvHalf<'_> {
    fn as_ref(&self) -> &UdpSocket {
        self.0
    }
}
