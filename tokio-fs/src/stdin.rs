use tokio_io::AsyncRead;

use std::io::{self, Read, Stdin as StdStdin};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// A handle to the standard input stream of a process.
///
/// The handle implements the [`AsyncRead`] trait, but beware that concurrent
/// reads of `Stdin` must be executed with care.
///
/// As an additional caveat, reading from the handle may block the calling
/// future indefinitely, if there is not enough data available. This makes this
/// handle unsuitable for use in any circumstance where immediate reaction to
/// available data is required, e.g. interactive use or when implementing a
/// subprocess driven by requests on the standard input.
///
/// Created by the [`stdin`] function.
///
/// [`stdin`]: fn.stdin.html
/// [`AsyncRead`]: trait.AsyncRead.html
#[derive(Debug)]
pub struct Stdin {
    std: StdStdin,
}

/// Constructs a new handle to the standard input of the current process.
///
/// The returned handle allows reading from standard input from the within the
/// Tokio runtime.
pub fn stdin() -> Stdin {
    let std = io::stdin();
    Stdin { std }
}

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        crate::would_block(|| self.std.read(buf))
    }
}

impl AsyncRead for Stdin {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::get_mut(self).read(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            other => Poll::Ready(other),
        }
    }
}
