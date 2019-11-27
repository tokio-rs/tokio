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

use crate::future::poll_fn;
use crate::net::udp::UdpSocket;

use std::error::Error;
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

/// The send half after [`split`](super::UdpSocket::split).
///
/// Use [`send_to`](#method.send_to) or [`send`](#method.send) to send
/// datagrams.
#[derive(Debug)]
pub struct SendHalf(Arc<UdpSocket>);

/// The recv half after [`split`](super::UdpSocket::split).
///
/// Use [`recv_from`](#method.recv_from) or [`recv`](#method.recv) to receive
/// datagrams.
#[derive(Debug)]
pub struct RecvHalf(Arc<UdpSocket>);

pub(crate) fn split(socket: UdpSocket) -> (RecvHalf, SendHalf) {
    let shared = Arc::new(socket);
    let send = shared.clone();
    let recv = shared;
    (RecvHalf(recv), SendHalf(send))
}

/// Error indicating two halves were not from the same socket, and thus could
/// not be `reunite`d.
#[derive(Debug)]
pub struct ReuniteError(pub SendHalf, pub RecvHalf);

impl fmt::Display for ReuniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tried to reunite halves that are not from the same socket"
        )
    }
}

impl Error for ReuniteError {}

fn reunite(s: SendHalf, r: RecvHalf) -> Result<UdpSocket, ReuniteError> {
    if Arc::ptr_eq(&s.0, &r.0) {
        drop(r);
        // Only two instances of the `Arc` are ever created, one for the
        // receiver and one for the sender, and those `Arc`s are never exposed
        // externally. And so when we drop one here, the other one must be the
        // only remaining one.
        Ok(Arc::try_unwrap(s.0).expect("udp: try_unwrap failed in reunite"))
    } else {
        Err(ReuniteError(s, r))
    }
}

impl RecvHalf {
    /// Attempts to put the two "halves" of a `UdpSocket` back together and
    /// recover the original socket. Succeeds only if the two "halves"
    /// originated from the same call to `UdpSocket::split`.
    pub fn reunite(self, other: SendHalf) -> Result<UdpSocket, ReuniteError> {
        reunite(other, self)
    }

    /// Returns a future that receives a single datagram on the socket. On success,
    /// the future resolves to the number of bytes read and the origin.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size
    /// to hold the message bytes. If a message is too long to fit in the supplied
    /// buffer, excess bytes may be discarded.
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        poll_fn(|cx| self.0.poll_recv_from(cx, buf)).await
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
        poll_fn(|cx| self.0.poll_recv(cx, buf)).await
    }
}

impl SendHalf {
    /// Attempts to put the two "halves" of a `UdpSocket` back together and
    /// recover the original socket. Succeeds only if the two "halves"
    /// originated from the same call to `UdpSocket::split`.
    pub fn reunite(self, other: RecvHalf) -> Result<UdpSocket, ReuniteError> {
        reunite(self, other)
    }

    /// Returns a future that sends data on the socket to the given address.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The future will resolve to an error if the IP version of the socket does
    /// not match that of `target`.
    pub async fn send_to(&mut self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        poll_fn(|cx| self.0.poll_send_to(cx, buf, target)).await
    }

    /// Returns a future that sends data on the socket to the remote address to which it is connected.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will resolve to an error if the socket is not connected.
    ///
    /// [`connect`]: super::UdpSocket::connect
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        poll_fn(|cx| self.0.poll_send(cx, buf)).await
    }
}

impl AsRef<UdpSocket> for SendHalf {
    fn as_ref(&self) -> &UdpSocket {
        &self.0
    }
}

impl AsRef<UdpSocket> for RecvHalf {
    fn as_ref(&self) -> &UdpSocket {
        &self.0
    }
}
