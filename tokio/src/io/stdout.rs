use crate::io::blocking::Blocking;
use crate::io::stdio_common::SplitByUtf8BoundaryIfWindows;
use crate::io::AsyncWrite;
use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

cfg_io_std! {
    /// A handle to the standard output stream of a process.
    ///
    /// Concurrent writes to stdout must be executed with care: Only individual
    /// writes to this [`AsyncWrite`] are guaranteed to be intact. In particular
    /// you should be aware that writes using [`write_all`] are not guaranteed
    /// to occur as a single write, so multiple threads writing data with
    /// [`write_all`] may result in interleaved output.
    ///
    /// Created by the [`stdout`] function.
    ///
    /// [`stdout`]: stdout()
    /// [`AsyncWrite`]: AsyncWrite
    /// [`write_all`]: crate::io::AsyncWriteExt::write_all()
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::{self, AsyncWriteExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut stdout = io::stdout();
    ///     stdout.write_all(b"Hello world!").await?;
    ///     Ok(())
    /// }
    /// ```
    #[derive(Debug)]
    pub struct Stdout {
        std: SplitByUtf8BoundaryIfWindows<Blocking<std::io::Stdout>>,
    }

    /// Constructs a new handle to the standard output of the current process.
    ///
    /// The returned handle allows writing to standard out from the within the
    /// Tokio runtime.
    ///
    /// Concurrent writes to stdout must be executed with care: Only individual
    /// writes to this [`AsyncWrite`] are guaranteed to be intact. In particular
    /// you should be aware that writes using [`write_all`] are not guaranteed
    /// to occur as a single write, so multiple threads writing data with
    /// [`write_all`] may result in interleaved output.
    ///
    /// [`AsyncWrite`]: AsyncWrite
    /// [`write_all`]: crate::io::AsyncWriteExt::write_all()
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::{self, AsyncWriteExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut stdout = io::stdout();
    ///     stdout.write_all(b"Hello world!").await?;
    ///     Ok(())
    /// }
    /// ```
    pub fn stdout() -> Stdout {
        let std = io::stdout();
        Stdout {
            std: SplitByUtf8BoundaryIfWindows::new(Blocking::new(std)),
        }
    }
}

#[cfg(unix)]
mod sys {
    #[cfg(not(tokio_no_as_fd))]
    use std::os::unix::io::{AsFd, BorrowedFd};
    use std::os::unix::io::{AsRawFd, RawFd};

    use super::Stdout;

    impl AsRawFd for Stdout {
        fn as_raw_fd(&self) -> RawFd {
            std::io::stdout().as_raw_fd()
        }
    }

    #[cfg(not(tokio_no_as_fd))]
    impl AsFd for Stdout {
        fn as_fd(&self) -> BorrowedFd<'_> {
            unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
        }
    }
}

#[cfg(windows)]
mod sys {
    #[cfg(not(tokio_no_as_fd))]
    use std::os::windows::io::{AsHandle, BorrowedHandle};
    use std::os::windows::io::{AsRawHandle, RawHandle};

    use super::Stdout;

    impl AsRawHandle for Stdout {
        fn as_raw_handle(&self) -> RawHandle {
            std::io::stdout().as_raw_handle()
        }
    }

    #[cfg(not(tokio_no_as_fd))]
    impl AsHandle for Stdout {
        fn as_handle(&self) -> BorrowedHandle<'_> {
            unsafe { BorrowedHandle::borrow_raw(self.as_raw_handle()) }
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
