//! `UnixStream` split support.
//!
//! A `UnixStream` can be split into a read half and a write half with
//! `UnixStream::split`. The read half implements `AsyncRead` while the write
//! half implements `AsyncWrite`.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split has no associated overhead and enforces all invariants at the type
//! level.

use crate::io::{AsyncRead, AsyncWrite};
use crate::net::UnixStream;

use std::io;
use std::mem::MaybeUninit;
use std::net::Shutdown;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Borrowed read half of a [`UnixStream`], created by [`split`].
///
/// Reading from a `ReadHalf` is usually done using the convenience methods found on the
/// [`AsyncReadExt`] trait. Examples import this trait through [the prelude].
///
/// [`UnixStream`]: UnixStream
/// [`split`]: UnixStream::split()
/// [`AsyncReadExt`]: trait@crate::io::AsyncReadExt
/// [the prelude]: crate::prelude
#[derive(Debug)]
pub struct ReadHalf<'a>(&'a UnixStream);

/// Borrowed write half of a [`UnixStream`], created by [`split`].
///
/// Note that in the [`AsyncWrite`] implemenation of this type, [`poll_shutdown`] will
/// shut down the UnixStream stream in the write direction.
///
/// Writing to an `WriteHalf` is usually done using the convenience methods found
/// on the [`AsyncWriteExt`] trait. Examples import this trait through [the prelude].
///
/// [`UnixStream`]: UnixStream
/// [`split`]: UnixStream::split()
/// [`AsyncWrite`]: trait@crate::io::AsyncWrite
/// [`poll_shutdown`]: fn@crate::io::AsyncWrite::poll_shutdown
/// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
/// [the prelude]: crate::prelude
#[derive(Debug)]
pub struct WriteHalf<'a>(&'a UnixStream);

pub(crate) fn split(stream: &mut UnixStream) -> (ReadHalf<'_>, WriteHalf<'_>) {
    (ReadHalf(stream), WriteHalf(stream))
}

impl AsyncRead for ReadHalf<'_> {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [MaybeUninit<u8>]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_read_priv(cx, buf)
    }
}

impl AsyncWrite for WriteHalf<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_priv(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.shutdown(Shutdown::Write).into()
    }
}

impl AsRef<UnixStream> for ReadHalf<'_> {
    fn as_ref(&self) -> &UnixStream {
        self.0
    }
}

impl AsRef<UnixStream> for WriteHalf<'_> {
    fn as_ref(&self) -> &UnixStream {
        self.0
    }
}
