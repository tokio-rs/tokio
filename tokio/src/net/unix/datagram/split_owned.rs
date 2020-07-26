//! `UnixDatagram` owned split support.
//!
//! A `UnixDatagram` can be split into an `OwnedSendHalf` and a `OwnedRecvHalf`
//! with the `UnixDatagram::into_split` method.

use crate::future::poll_fn;
use crate::net::UnixDatagram;

use std::error::Error;
use std::net::Shutdown;
use std::os::unix::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::{fmt, io};

pub(crate) fn split_owned(socket: UnixDatagram) -> (OwnedRecvHalf, OwnedSendHalf) {
    let shared = Arc::new(socket);
    let send = shared.clone();
    let recv = shared;
    (
        OwnedRecvHalf { inner: recv },
        OwnedSendHalf {
            inner: send,
            shutdown_on_drop: true,
        },
    )
}

/// Owned send half of a [`UnixDatagram`], created by [`into_split`].
///
/// [`UnixDatagram`]: UnixDatagram
/// [`into_split`]: UnixDatagram::into_split()
#[derive(Debug)]
pub struct OwnedSendHalf {
    inner: Arc<UnixDatagram>,
    shutdown_on_drop: bool,
}

/// Owned receive half of a [`UnixDatagram`], created by [`into_split`].
///
/// [`UnixDatagram`]: UnixDatagram
/// [`into_split`]: UnixDatagram::into_split()
#[derive(Debug)]
pub struct OwnedRecvHalf {
    inner: Arc<UnixDatagram>,
}

/// Error indicating that two halves were not from the same socket, and thus could
/// not be `reunite`d.
#[derive(Debug)]
pub struct ReuniteError(pub OwnedSendHalf, pub OwnedRecvHalf);

impl fmt::Display for ReuniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tried to reunite halves that are not from the same socket"
        )
    }
}

impl Error for ReuniteError {}

fn reunite(s: OwnedSendHalf, r: OwnedRecvHalf) -> Result<UnixDatagram, ReuniteError> {
    if Arc::ptr_eq(&s.inner, &r.inner) {
        s.forget();
        // Only two instances of the `Arc` are ever created, one for the
        // receiver and one for the sender, and those `Arc`s are never exposed
        // externally. And so when we drop one here, the other one must be the
        // only remaining one.
        Ok(Arc::try_unwrap(r.inner).expect("UnixDatagram: try_unwrap failed in reunite"))
    } else {
        Err(ReuniteError(s, r))
    }
}

impl OwnedRecvHalf {
    /// Attempts to put the two "halves" of a `UnixDatagram` back together and
    /// recover the original socket. Succeeds only if the two "halves"
    /// originated from the same call to [`into_split`].
    ///
    /// [`into_split`]: UnixDatagram::into_split()
    pub fn reunite(self, other: OwnedSendHalf) -> Result<UnixDatagram, ReuniteError> {
        reunite(other, self)
    }

    /// Receives data from the socket.
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        poll_fn(|cx| self.inner.poll_recv_from_priv(cx, buf)).await
    }

    /// Receives data from the socket.
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.inner.poll_recv_priv(cx, buf)).await
    }
}

impl OwnedSendHalf {
    /// Attempts to put the two "halves" of a `UnixDatagram` back together and
    /// recover the original socket. Succeeds only if the two "halves"
    /// originated from the same call to [`into_split`].
    ///
    /// [`into_split`]: UnixDatagram::into_split()
    pub fn reunite(self, other: OwnedRecvHalf) -> Result<UnixDatagram, ReuniteError> {
        reunite(self, other)
    }

    /// Sends data on the socket to the specified address.
    pub async fn send_to<P>(&mut self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path> + Unpin,
    {
        poll_fn(|cx| self.inner.poll_send_to_priv(cx, buf, target.as_ref())).await
    }

    /// Sends data on the socket to the socket's peer.
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        poll_fn(|cx| self.inner.poll_send_priv(cx, buf)).await
    }

    /// Destroy the send half, but don't close the send half of the stream
    /// until the receive half is dropped. If the read half has already been
    /// dropped, this closes the stream.
    pub fn forget(mut self) {
        self.shutdown_on_drop = false;
        drop(self);
    }
}

impl Drop for OwnedSendHalf {
    fn drop(&mut self) {
        if self.shutdown_on_drop {
            let _ = self.inner.shutdown(Shutdown::Write);
        }
    }
}

impl AsRef<UnixDatagram> for OwnedSendHalf {
    fn as_ref(&self) -> &UnixDatagram {
        &self.inner
    }
}

impl AsRef<UnixDatagram> for OwnedRecvHalf {
    fn as_ref(&self) -> &UnixDatagram {
        &self.inner
    }
}
