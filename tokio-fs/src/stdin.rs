use tokio_io::{AsyncRead};

use std::io::{self, Read, Stdin as StdStdin};

/// A handle to the standard input stream of a process.
///
/// The handle implements the [`AsyncRead`] trait, but beware that concurrent
/// reads of `Stdin` must be executed with care.
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
        ::would_block(|| self.std.read(buf))
    }
}

impl AsyncRead for Stdin {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }
}
