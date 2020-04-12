use crate::io::blocking::Blocking;
use crate::io::AsyncRead;

use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

cfg_io_std! {
    /// A handle to the standard input stream of a process.
    ///
    /// The handle implements the [`AsyncRead`] trait, but beware that concurrent
    /// reads of `Stdin` must be executed with care.
    ///
    /// As an additional caveat, reading from the handle may block the calling
    /// future indefinitely if there is not enough data available. This makes this
    /// handle unsuitable for use in any circumstance where immediate reaction to
    /// available data is required, e.g. interactive use or when implementing a
    /// subprocess driven by requests on the standard input.
    ///
    /// Created by the [`stdin`] function.
    ///
    /// [`stdin`]: fn@stdin
    /// [`AsyncRead`]: trait@AsyncRead
    #[derive(Debug)]
    pub struct Stdin {
        std: Blocking<std::io::Stdin>,
    }

    /// Constructs a new handle to the standard input of the current process.
    ///
    /// The returned handle allows reading from standard input from the within the
    /// Tokio runtime.
    ///
    /// As an additional caveat, reading from the handle may block the calling
    /// future indefinitely if there is not enough data available. This makes this
    /// handle unsuitable for use in any circumstance where immediate reaction to
    /// available data is required, e.g. interactive use or when implementing a
    /// subprocess driven by requests on the standard input.
    pub fn stdin() -> Stdin {
        let std = io::stdin();
        Stdin {
            std: Blocking::new(std),
        }
    }
}

#[cfg(unix)]
impl std::os::unix::io::AsRawFd for Stdin {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        std::io::stdin().as_raw_fd()
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for Stdin {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        std::io::stdin().as_raw_handle()
    }
}

impl AsyncRead for Stdin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.std).poll_read(cx, buf)
    }
}
