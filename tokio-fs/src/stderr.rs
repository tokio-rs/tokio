use crate::blocking_io;

use tokio_io::AsyncWrite;

use std::io::{self, Write};
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
    std: std::io::Stderr,
}

/// Constructs a new handle to the standard error of the current process.
///
/// The returned handle allows writing to standard error from the within the
/// Tokio runtime.
pub fn stderr() -> Stderr {
    let std = io::stderr();
    Stderr { std }
}

impl AsyncWrite for Stderr {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        blocking_io(|| (&mut self.std).write(buf))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        blocking_io(|| (&mut self.std).flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}
