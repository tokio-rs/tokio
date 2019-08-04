use crate::blocking_io;

use tokio_io::AsyncWrite;

use std::io::{self, Write};
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
    std: std::io::Stdout,
}

/// Constructs a new handle to the standard output of the current process.
///
/// The returned handle allows writing to standard out from the within the Tokio
/// runtime.
pub fn stdout() -> Stdout {
    let std = io::stdout();
    Stdout { std }
}

impl AsyncWrite for Stdout {
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
