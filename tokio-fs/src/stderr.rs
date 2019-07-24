use tokio_io::AsyncWrite;

use std::io::{self, Stderr as StdStderr, Write};
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
    std: StdStderr,
}

/// Constructs a new handle to the standard error of the current process.
///
/// The returned handle allows writing to standard error from the within the
/// Tokio runtime.
pub fn stderr() -> Stderr {
    let std = io::stderr();
    Stderr { std }
}

impl Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        crate::would_block(|| self.std.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        crate::would_block(|| self.std.flush())
    }
}

impl AsyncWrite for Stderr {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::get_mut(self).write(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            other => Poll::Ready(other),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match Pin::get_mut(self).flush() {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            other => Poll::Ready(other),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}
