use tokio_io::AsyncWrite;

use std::io::{self, Stdout as StdStdout, Write};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// A handle to the standard output stream of a process.
///
/// The handle implements the [`AsyncWrite`] trait, but beware that concurrent
/// writes to `Stdout` must be executed with care.
///
/// Created by the [`stdout`] function.
///
/// [`stdout`]: fn.stdout.html
/// [`AsyncWrite`]: trait.AsyncWrite.html
#[derive(Debug)]
pub struct Stdout {
    std: StdStdout,
}

/// Constructs a new handle to the standard output of the current process.
///
/// The returned handle allows writing to standard out from the within the Tokio
/// runtime.
pub fn stdout() -> Stdout {
    let std = io::stdout();
    Stdout { std }
}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        crate::would_block(|| self.std.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        crate::would_block(|| self.std.flush())
    }
}

impl AsyncWrite for Stdout {
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
