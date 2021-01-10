//! `UnixStream` owned split support.
//!
//! A `UnixStream` can be split into an `OwnedReadHalf` and a `OwnedWriteHalf`
//! with the `UnixStream::into_split` method.  `OwnedReadHalf` implements
//! `AsyncRead` while `OwnedWriteHalf` implements `AsyncWrite`.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split has no associated overhead and enforces all invariants at the type
//! level.

use crate::io::{AsyncRead, AsyncWrite, ReadBuf};
use crate::net::UnixStream;

use std::error::Error;
use std::net::Shutdown;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{fmt, io};

/// Owned read half of a [`UnixStream`], created by [`into_split`].
///
/// Reading from an `OwnedReadHalf` is usually done using the convenience methods found
/// on the [`AsyncReadExt`] trait.
///
/// [`UnixStream`]: crate::net::UnixStream
/// [`into_split`]: crate::net::UnixStream::into_split()
/// [`AsyncReadExt`]: trait@crate::io::AsyncReadExt
#[derive(Debug)]
pub struct OwnedReadHalf {
    inner: Arc<UnixStream>,
}

/// Owned write half of a [`UnixStream`], created by [`into_split`].
///
/// Note that in the [`AsyncWrite`] implementation of this type,
/// [`poll_shutdown`] will shut down the stream in the write direction.
/// Dropping the write half will also shut down the write half of the stream.
///
/// Writing to an `OwnedWriteHalf` is usually done using the convenience methods
/// found on the [`AsyncWriteExt`] trait.
///
/// [`UnixStream`]: crate::net::UnixStream
/// [`into_split`]: crate::net::UnixStream::into_split()
/// [`AsyncWrite`]: trait@crate::io::AsyncWrite
/// [`poll_shutdown`]: fn@crate::io::AsyncWrite::poll_shutdown
/// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
#[derive(Debug)]
pub struct OwnedWriteHalf {
    inner: Arc<UnixStream>,
    shutdown_on_drop: bool,
}

pub(crate) fn split_owned(stream: UnixStream) -> (OwnedReadHalf, OwnedWriteHalf) {
    let arc = Arc::new(stream);
    let read = OwnedReadHalf {
        inner: Arc::clone(&arc),
    };
    let write = OwnedWriteHalf {
        inner: arc,
        shutdown_on_drop: true,
    };
    (read, write)
}

pub(crate) fn reunite(
    read: OwnedReadHalf,
    write: OwnedWriteHalf,
) -> Result<UnixStream, ReuniteError> {
    if Arc::ptr_eq(&read.inner, &write.inner) {
        write.forget();
        // This unwrap cannot fail as the api does not allow creating more than two Arcs,
        // and we just dropped the other half.
        Ok(Arc::try_unwrap(read.inner).expect("UnixStream: try_unwrap failed in reunite"))
    } else {
        Err(ReuniteError(read, write))
    }
}

/// Error indicating that two halves were not from the same socket, and thus could
/// not be reunited.
#[derive(Debug)]
pub struct ReuniteError(pub OwnedReadHalf, pub OwnedWriteHalf);

impl fmt::Display for ReuniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tried to reunite halves that are not from the same socket"
        )
    }
}

impl Error for ReuniteError {}

impl OwnedReadHalf {
    /// Attempts to put the two halves of a `UnixStream` back together and
    /// recover the original socket. Succeeds only if the two halves
    /// originated from the same call to [`into_split`].
    ///
    /// [`into_split`]: crate::net::UnixStream::into_split()
    pub fn reunite(self, other: OwnedWriteHalf) -> Result<UnixStream, ReuniteError> {
        reunite(self, other)
    }
}

impl AsyncRead for OwnedReadHalf {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.inner.poll_read_priv(cx, buf)
    }
}

impl OwnedWriteHalf {
    /// Attempts to put the two halves of a `UnixStream` back together and
    /// recover the original socket. Succeeds only if the two halves
    /// originated from the same call to [`into_split`].
    ///
    /// [`into_split`]: crate::net::UnixStream::into_split()
    pub fn reunite(self, other: OwnedReadHalf) -> Result<UnixStream, ReuniteError> {
        reunite(other, self)
    }

    /// Destroy the write half, but don't close the write half of the stream
    /// until the read half is dropped. If the read half has already been
    /// dropped, this closes the stream.
    pub fn forget(mut self) {
        self.shutdown_on_drop = false;
        drop(self);
    }
}

impl Drop for OwnedWriteHalf {
    fn drop(&mut self) {
        if self.shutdown_on_drop {
            let _ = self.inner.shutdown_std(Shutdown::Write);
        }
    }
}

impl AsyncWrite for OwnedWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_write_priv(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_write_vectored_priv(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // flush is a no-op
        Poll::Ready(Ok(()))
    }

    // `poll_shutdown` on a write half shutdowns the stream in the "write" direction.
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        let res = self.inner.shutdown_std(Shutdown::Write);
        if res.is_ok() {
            Pin::into_inner(self).shutdown_on_drop = false;
        }
        res.into()
    }
}

impl AsRef<UnixStream> for OwnedReadHalf {
    fn as_ref(&self) -> &UnixStream {
        &*self.inner
    }
}

impl AsRef<UnixStream> for OwnedWriteHalf {
    fn as_ref(&self) -> &UnixStream {
        &*self.inner
    }
}
