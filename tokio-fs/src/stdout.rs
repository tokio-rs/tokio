use tokio_io::{AsyncWrite};

use futures::Poll;

use std::io::{self, Write, Stdout as StdStdout};

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
        ::would_block(|| self.std.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        ::would_block(|| self.std.flush())
    }
}

impl AsyncWrite for Stdout {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }
}
