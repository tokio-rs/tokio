use crate::io::blocking::Blocking;
use crate::io::{AsyncRead, ReadBuf};

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
    /// This handle is best used for non-interactive uses, such as when a file
    /// is piped into the application. For technical reasons, `stdin` is
    /// implemented by using an ordinary blocking read on a separate thread, and
    /// it is impossible to cancel that read. This can make shutdown of the
    /// runtime hang until the user presses enter.
    ///
    /// For interactive uses, it is recommended to spawn a thread dedicated to
    /// user input and use blocking IO directly in that thread.
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
    /// This handle is best used for non-interactive uses, such as when a file
    /// is piped into the application. For technical reasons, `stdin` is
    /// implemented by using an ordinary blocking read on a separate thread, and
    /// it is impossible to cancel that read. This can make shutdown of the
    /// runtime hang until the user presses enter.
    ///
    /// For interactive uses, it is recommended to spawn a thread dedicated to
    /// user input and use blocking IO directly in that thread.
    ///
    /// # Examples
    ///
    /// Reading all data piped into the process from standard input. This is the
    /// recommended, non-interactive use of [`Stdin`]: when input is piped in,
    /// stdin reaches end-of-file and the read completes, so the runtime can shut
    /// down cleanly.
    ///
    /// ```no_run
    /// use tokio::io::{self, AsyncReadExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut stdin = io::stdin();
    ///     let mut buf = Vec::new();
    ///     stdin.read_to_end(&mut buf).await?;
    ///
    ///     println!("read {} bytes from stdin", buf.len());
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Handling *interactive* input. Reading [`Stdin`] directly from async code
    /// is not recommended: because the read is performed by an uncancellable
    /// blocking operation, awaiting it can make runtime shutdown hang until the
    /// user presses enter. Instead, spawn a dedicated thread that performs the
    /// blocking reads and forwards each line over a channel. Async code then
    /// selects over that channel, so it can stop on a shutdown signal without
    /// waiting for the blocking read to finish:
    ///
    /// ```no_run
    /// use std::io::BufRead;
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = mpsc::channel::<String>(16);
    ///
    ///     // Blocking, uncancellable reads live on this dedicated thread, off
    ///     // the async runtime. Each line is forwarded to async code.
    ///     std::thread::spawn(move || {
    ///         for line in std::io::stdin().lock().lines() {
    ///             let line = match line {
    ///                 Ok(line) => line,
    ///                 Err(_) => break,
    ///             };
    ///             // The receiver is dropped once the runtime shuts down; stop.
    ///             if tx.blocking_send(line).is_err() {
    ///                 break;
    ///             }
    ///         }
    ///     });
    ///
    ///     loop {
    ///         tokio::select! {
    ///             maybe_line = rx.recv() => match maybe_line {
    ///                 Some(line) => println!("read line: {}", line),
    ///                 None => break, // input thread reached end-of-file
    ///             },
    ///             _ = tokio::signal::ctrl_c() => {
    ///                 // Stop awaiting input so the runtime can shut down, even
    ///                 // though the reader thread may still be blocked on read.
    ///                 println!("shutting down");
    ///                 break;
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn stdin() -> Stdin {
        let std = io::stdin();
        // SAFETY: The `Read` implementation of `std` does not read from the
        // buffer it is borrowing and correctly reports the length of the data
        // written into the buffer.
        let std = unsafe { Blocking::new(std) };
        Stdin {
            std,
        }
    }
}

#[cfg(unix)]
mod sys {
    use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, RawFd};

    use super::Stdin;

    impl AsRawFd for Stdin {
        fn as_raw_fd(&self) -> RawFd {
            std::io::stdin().as_raw_fd()
        }
    }

    impl AsFd for Stdin {
        fn as_fd(&self) -> BorrowedFd<'_> {
            unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
        }
    }
}

cfg_windows! {
    use crate::os::windows::io::{AsHandle, BorrowedHandle, AsRawHandle, RawHandle};

    impl AsRawHandle for Stdin {
        fn as_raw_handle(&self) -> RawHandle {
            std::io::stdin().as_raw_handle()
        }
    }

    impl AsHandle for Stdin {
        fn as_handle(&self) -> BorrowedHandle<'_> {
            unsafe { BorrowedHandle::borrow_raw(self.as_raw_handle()) }
        }
    }
}

impl AsyncRead for Stdin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.std).poll_read(cx, buf)
    }
}
