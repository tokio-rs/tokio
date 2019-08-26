use crate::blocking::Blocking;

use tokio_io::AsyncWrite;

use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// A handle to the standard error stream of a process.
///
/// The handle implements the [`AsyncWrite`] trait, but beware that concurrent
/// writes to `Stderr` must be executed with care.
///
/// Created by the [`stderr`] function.
///
/// [`stderr`]: fn.stderr.html
/// [`AsyncWrite`]: trait.AsyncWrite.html
#[derive(Debug)]
pub struct Stderr {
    std: Blocking<std::io::Stderr>,
}

/// Constructs a new handle to the standard error of the current process.
///
/// The returned handle allows writing to standard error from the within the
/// Tokio runtime.
pub fn stderr() -> Stderr {
    let std = io::stderr();
    Stderr {
        std: Blocking::new(std),
    }
}

impl AsyncWrite for Stderr {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.std).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.std).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.std).poll_shutdown(cx)
    }
}
