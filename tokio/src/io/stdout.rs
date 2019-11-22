use crate::io::blocking::Blocking;
use crate::io::AsyncWrite;

use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

cfg_io_std! {
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
        std: Blocking<std::io::Stdout>,
    }

    /// Constructs a new handle to the standard output of the current process.
    ///
    /// The returned handle allows writing to standard out from the within the Tokio
    /// runtime.
    pub fn stdout() -> Stdout {
        let std = io::stdout();
        Stdout {
            std: Blocking::new(std),
        }
    }
}

impl AsyncWrite for Stdout {
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
