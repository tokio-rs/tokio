use tokio_io::{AsyncWrite};

use futures::Poll;

use std::io::{self, Write, Stderr as StdStderr};

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
        ::would_block(|| self.std.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        ::would_block(|| self.std.flush())
    }
}

impl AsyncWrite for Stderr {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }
}

