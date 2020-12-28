//! `UdpSocket` owned split support.
//!
//! A `UdpSocket` can be split into an `OwnedRecvHalf` and a `OwnedSendHalf`
//! with the `UdpSocket::into_split` method.  `OwnedRecvHalf` can call
//! `recv`/`recv_from` while `OwnedSendHalf` can call `send`/`send_to`
//!
//! If you need to use a `UdpSocket` concurrently from more than 2 separate tasks,
//! you're free to `Arc` the `UdpSocket` yourself and `clone` as much as you
//! need. All it's async method are safe to use from multiple tasks.

use crate::net::{ToSocketAddrs, UdpSocket};

use std::{error::Error, fmt, io, net::SocketAddr, sync::Arc};

/// Owned read half of a [`UdpSocket`], created by [`into_split`].
///
/// Reading from an `OwnedRecvHalf` is done using `recv` or `recv_from`
///
/// [`UdpSocket`]: crate::net::UdpSocket
/// [`into_split`]: crate::net::UdpSocket::into_split()
#[derive(Debug)]
pub struct OwnedRecvHalf(Arc<UdpSocket>);

/// Owned write half of a [`UdpSocket`], created by [`into_split`].
///
/// Write to `OwnedSendHalf` is done using `send` or `send_to`
///
/// [`UdpSocket`]: crate::net::UdpSocket
/// [`into_split`]: crate::net::UdpSocket::into_split()
#[derive(Debug)]
pub struct OwnedSendHalf(Arc<UdpSocket>);

pub(crate) fn split_owned(stream: UdpSocket) -> (OwnedRecvHalf, OwnedSendHalf) {
    let arc = Arc::new(stream);
    (OwnedRecvHalf(Arc::clone(&arc)), OwnedSendHalf(arc))
}

pub(crate) fn reunite(recv: OwnedRecvHalf, send: OwnedSendHalf) -> Result<UdpSocket, ReuniteError> {
    if Arc::ptr_eq(&recv.0, &send.0) {
        drop(recv);
        // This unwrap cannot fail as the api does not allow creating more than two Arcs,
        // and we just dropped the other half.
        Ok(Arc::try_unwrap(send.0).expect("UdpSocket: try_unwrap failed in reunite"))
    } else {
        Err(ReuniteError(recv, send))
    }
}

/// Error indicating that two halves were not from the same socket, and thus could
/// not be reunited.
#[derive(Debug)]
pub struct ReuniteError(pub OwnedRecvHalf, pub OwnedSendHalf);

impl fmt::Display for ReuniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tried to reunite halves that are not from the same socket"
        )
    }
}

impl Error for ReuniteError {}

impl OwnedRecvHalf {
    /// Attempts to put the two halves of a `UdpSocket` back together and
    /// recover the original socket. Succeeds only if the two halves
    /// originated from the same call to [`into_split`].
    ///
    /// [`into_split`]: crate::net::UdpSocket::into_split()
    pub fn reunite(self, other: OwnedSendHalf) -> Result<UdpSocket, ReuniteError> {
        reunite(self, other)
    }
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

impl OwnedSendHalf {
    /// Attempts to put the two halves of a `UdpSocket` back together and
    /// recover the original socket. Succeeds only if the two halves
    /// originated from the same call to [`into_split`].
    ///
    /// [`into_split`]: crate::net::UdpSocket::into_split()
    pub fn reunite(self, other: OwnedRecvHalf) -> Result<UdpSocket, ReuniteError> {
        reunite(other, self)
    }

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

impl AsRef<UdpSocket> for OwnedRecvHalf {
    fn as_ref(&self) -> &UdpSocket {
        &*self.0
    }
}

impl AsRef<UdpSocket> for OwnedSendHalf {
    fn as_ref(&self) -> &UdpSocket {
        &*self.0
    }
}
