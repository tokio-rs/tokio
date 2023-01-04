//! Tokio support for Unix pipes.

use mio::unix::pipe as mio_pipe;
use std::convert::TryFrom;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::prelude::OpenOptionsExt;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::io::interest::Interest;
use crate::io::{AsyncRead, AsyncWrite, PollEvented, ReadBuf};

cfg_net_unix! {
    /// Writing end of a Unix pipe.
    #[derive(Debug)]
    pub struct Sender {
        io: PollEvented<mio_pipe::Sender>,
    }
}

impl Sender {
    /// Open a writing end of a pipe from a FIFO file.
    ///
    /// This function will open the file at the specified path, check if the file
    /// is a FIFO file and associate the pipe with the default event loop's handles
    /// for writing.
    ///
    /// This function will fail with an OS error if there are no reading ends open.
    /// On Linux you can use [`open_dangling`] to work around this.
    ///
    /// [`open_dangling`]: Self::open_dangling
    ///
    /// # Errors
    ///
    /// Returns an error if the specified file is not a FIFO file.
    /// Will also result in an error if called outside of a [Tokio Runtime], or in
    /// a runtime that has not [enabled I/O], or if any OS-specific I/O errors occur.
    ///
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [enabled I/O]: crate::runtime::Builder::enable_io
    pub fn open<P>(path: P) -> io::Result<Sender>
    where
        P: AsRef<Path>,
    {
        Sender::open_with_read_access(path.as_ref(), false)
    }

    /// Open a writing end of a pipe from a FIFO file without a present reader.
    ///
    /// This function will open the file at the specified path, check if the file
    /// is a FIFO file and associate the pipe with the default event loop's handles
    /// for writing.
    ///
    /// Unlike [`open`], this will not error if there is no open reading end of the FIFO.
    /// This is done by opening the FIFO file with access for both reading and writing.
    /// Note that behavior of such operation is not defined by POSIX and is only
    /// guaranteed to work on Linux.
    ///
    /// [`open`]: Self::open
    ///
    /// # Errors
    ///
    /// Returns an error if the specified file is not a FIFO file.
    /// Will also result in an error if called outside of a [Tokio Runtime], or in
    /// a runtime that has not [enabled I/O], or if any OS-specific I/O errors occur.
    ///
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [enabled I/O]: crate::runtime::Builder::enable_io
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tokio::io::AsyncWriteExt;
    /// # use tokio::net::pipe::Sender;
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let new_fifo_path = dir.path().join("fifo");
    /// // Create a new FIFO file.
    /// nix::unistd::mkfifo(&new_fifo_path, nix::sys::stat::Mode::S_IRWXU)?;
    ///
    /// // `Sender::open` would fail here, since there is no open reading end.
    /// let mut tx = Sender::open_dangling(&new_fifo_path)?;
    /// // We can asynchronously write to the pipe before any reader.
    /// tx.write_all(b"hello world").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn open_dangling<P>(path: P) -> io::Result<Sender>
    where
        P: AsRef<Path>,
    {
        Sender::open_with_read_access(path.as_ref(), true)
    }

    fn open_with_read_access(path: &Path, read_access: bool) -> io::Result<Sender> {
        let file = OpenOptions::new()
            .read(read_access)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        if file.metadata()?.file_type().is_fifo() {
            let raw_fd = file.into_raw_fd();
            // Safety: We have just created the raw fd from a valid fifo file.
            let pipe = unsafe { mio_pipe::Sender::from_raw_fd(raw_fd) };
            Sender::from_mio(pipe)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "file is not a fifo"))
        }
    }

    fn from_mio(pipe: mio_pipe::Sender) -> io::Result<Sender> {
        let io = PollEvented::new_with_interest(pipe, Interest::WRITABLE)?;
        Ok(Sender { io })
    }

    /// Waits for the pipe to become writable.
    ///
    /// This function is usually paired with [`try_write()`].
    ///
    /// [`try_write()`]: Self::try_write
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Sender;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a writing end of a fifo
    ///     let pipe = Sender::open("path/to/a/fifo")?;
    ///
    ///     loop {
    ///         // Wait for the pipe to be writable
    ///         pipe.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match pipe.try_write(b"hello world") {
    ///             Ok(n) => {
    ///                 break;
    ///             }
    ///             Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn writable(&self) -> io::Result<()> {
        self.io.registration().readiness(Interest::WRITABLE).await?;
        Ok(())
    }

    /// Polls for write readiness.
    ///
    /// If the pipe is not currently ready for writing, this method will
    /// store a clone of the `Waker` from the provided `Context`. When the pipe
    /// becomes ready for writing, `Waker::wake` will be called on the waker.
    ///
    /// Note that on multiple calls to `poll_write_ready` or `poll_write`, only
    /// the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup. (However, `poll_read_ready` retains a
    /// second, independent waker.)
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`writable`] is not feasible. Where possible, using [`writable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    ///
    /// [`writable`]: Self::writable
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the pipe is not ready for writing.
    /// * `Poll::Ready(Ok(()))` if the pipe is ready for writing.
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.registration().poll_write_ready(cx).map_ok(|_| ())
    }

    /// Tries to write a buffer to the pipe, returning how many bytes were
    /// written.
    ///
    /// The function will attempt to write the entire contents of `buf`, but
    /// only part of the buffer may be written.
    ///
    /// This function is usually paired with [`writable`].
    ///
    /// [`writable`]: Self::writable
    ///
    /// # Return
    ///
    /// If data is successfully written, `Ok(n)` is returned, where `n` is the
    /// number of bytes written. If the pipe is not ready to write data,
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Sender;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a writing end of a fifo
    ///     let pipe = Sender::open("path/to/a/fifo")?;
    ///
    ///     loop {
    ///         // Wait for the pipe to be writable
    ///         pipe.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match pipe.try_write(b"hello world") {
    ///             Ok(n) => {
    ///                 break;
    ///             }
    ///             Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::WRITABLE, || (&*self.io).write(buf))
    }

    /// Tries to write several buffers to the pipe, returning how many bytes
    /// were written.
    ///
    /// Data is written from each buffer in order, with the final buffer read
    /// from possible being only partially consumed. This method behaves
    /// equivalently to a single call to [`try_write()`] with concatenated
    /// buffers.
    ///
    /// This function is usually paired with [`writable`].
    ///
    /// [`try_write()`]: Self::try_write()
    /// [`writable`]: Self::writable
    ///
    /// # Return
    ///
    /// If data is successfully written, `Ok(n)` is returned, where `n` is the
    /// number of bytes written. If the stream is not ready to write data,
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Sender;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a writing end of a fifo
    ///     let pipe = Sender::open("path/to/a/fifo")?;
    ///
    ///     let bufs = [io::IoSlice::new(b"hello "), io::IoSlice::new(b"world")];
    ///
    ///     loop {
    ///         // Wait for the pipe to be writable
    ///         pipe.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match pipe.try_write_vectored(&bufs) {
    ///             Ok(n) => {
    ///                 break;
    ///             }
    ///             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_write_vectored(&self, buf: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::WRITABLE, || (&*self.io).write_vectored(buf))
    }
}

impl AsyncWrite for Sender {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsRawFd for Sender {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

cfg_process! {
    use crate::process::ChildStdin;

    impl TryFrom<ChildStdin> for Sender {
        type Error = io::Error;
        fn try_from(stdin: ChildStdin) -> io::Result<Sender> {
            // Safety: ChildStdin has a valid fd to the writing end of a pipe.
            let mio_tx = unsafe { mio_pipe::Sender::from_raw_fd(stdin.into_fd()?) };
            Sender::from_mio(mio_tx)
        }
    }
}

cfg_net_unix! {
    /// Reading end of a Unix pipe.
    #[derive(Debug)]
    pub struct Receiver {
        io: PollEvented<mio_pipe::Receiver>,
    }
}

impl Receiver {
    /// Open a reading end of a pipe from a FIFO file.
    ///
    /// This function will open the file at the specified path, check if the file
    /// is a FIFO file and associate the pipe with the default event loop's handles
    /// for reading.
    ///
    /// # Errors
    ///
    /// Returns an error if the specified file is not a FIFO file.
    /// Will also result in an error if called outside of a [Tokio Runtime], or in
    /// a runtime that has not [enabled I/O], or if any OS-specific I/O errors occur.
    ///
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [enabled I/O]: crate::runtime::Builder::enable_io
    pub fn open<P>(path: P) -> io::Result<Receiver>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        if file.metadata()?.file_type().is_fifo() {
            let raw_fd = file.into_raw_fd();
            // Safety: We just created the raw fd from a valid fifo file.
            let pipe = unsafe { mio_pipe::Receiver::from_raw_fd(raw_fd) };
            Receiver::from_mio(pipe)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "file is not a fifo"))
        }
    }

    fn from_mio(pipe: mio_pipe::Receiver) -> io::Result<Receiver> {
        let io = PollEvented::new_with_interest(pipe, Interest::READABLE)?;
        Ok(Receiver { io })
    }

    /// Waits for the pipe to become readable.
    ///
    /// This function is usually paired with [`try_read()`].
    ///
    /// [`try_read()`]: Self::try_read()
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Receiver;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a reading end of a fifo
    ///     let pipe = Receiver::open("path/to/a/fifo")?;
    ///
    ///     let mut msg = vec![0; 1024];
    ///
    ///     loop {
    ///         // Wait for the pipe to be readable
    ///         pipe.readable().await?;
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match pipe.try_read(&mut msg) {
    ///             Ok(n) => {
    ///                 msg.truncate(n);
    ///                 break;
    ///             }
    ///             Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     println!("GOT = {:?}", msg);
    ///     Ok(())
    /// }
    /// ```
    pub async fn readable(&self) -> io::Result<()> {
        self.io.registration().readiness(Interest::READABLE).await?;
        Ok(())
    }

    /// Polls for read readiness.
    ///
    /// If the pipe is not currently ready for reading, this method will
    /// store a clone of the `Waker` from the provided `Context`. When the pipe
    /// becomes ready for reading, `Waker::wake` will be called on the waker.
    ///
    /// Note that on multiple calls to `poll_read_ready` or `poll_read`, only
    /// the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup. (However, `poll_write_ready` retains a
    /// second, independent waker.)
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`readable`] is not feasible. Where possible, using [`readable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    ///
    /// [`readable`]: Self::readable
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the pipe is not ready for reading.
    /// * `Poll::Ready(Ok(()))` if the pipe is ready for reading.
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.registration().poll_read_ready(cx).map_ok(|_| ())
    }

    /// Tries to read data from the pipe into the provided buffer, returning how
    /// many bytes were read.
    ///
    /// Receives any pending data from the pipe but does not wait for new data
    /// to arrive. On success, returns the number of bytes read. Because
    /// `try_read()` is non-blocking, the buffer does not have to be stored by
    /// the async task and can exist entirely on the stack.
    ///
    /// Usually [`readable()`] is used with this function.
    ///
    /// [`readable()`]: Self::readable()
    ///
    /// # Return
    ///
    /// If data is successfully read, `Ok(n)` is returned, where `n` is the
    /// number of bytes read. If `n` is `0`, then it can indicate one of two scenarios:
    ///
    /// 1. The pipe's read half is closed and will no longer yield data.
    /// 2. The specified buffer was 0 bytes in length.
    ///
    /// If the pipe is not ready to read data,
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Receiver;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a reading end of a fifo
    ///     let pipe = Receiver::open("path/to/a/fifo")?;
    ///
    ///     let mut msg = vec![0; 1024];
    ///
    ///     loop {
    ///         // Wait for the pipe to be readable
    ///         pipe.readable().await?;
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match pipe.try_read(&mut msg) {
    ///             Ok(n) => {
    ///                 msg.truncate(n);
    ///                 break;
    ///             }
    ///             Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     println!("GOT = {:?}", msg);
    ///     Ok(())
    /// }
    /// ```
    pub fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::READABLE, || (&*self.io).read(buf))
    }

    /// Tries to read data from the pipe into the provided buffers, returning
    /// how many bytes were read.
    ///
    /// Data is copied to fill each buffer in order, with the final buffer
    /// written to possibly being only partially filled. This method behaves
    /// equivalently to a single call to [`try_read()`] with concatenated
    /// buffers.
    ///
    /// Receives any pending data from the pipe but does not wait for new data
    /// to arrive. On success, returns the number of bytes read. Because
    /// `try_read_vectored()` is non-blocking, the buffer does not have to be
    /// stored by the async task and can exist entirely on the stack.
    ///
    /// Usually, [`readable()`] is used with this function.
    ///
    /// [`try_read()`]: Self::try_read()
    /// [`readable()`]: Self::readable()
    ///
    /// # Return
    ///
    /// If data is successfully read, `Ok(n)` is returned, where `n` is the
    /// number of bytes read. `Ok(0)` indicates the pipe's read half is closed
    /// and will no longer yield data. If the pipe is not ready to read data
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Receiver;
    /// use std::error::Error;
    /// use std::io::{self, IoSliceMut};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a reading end of a fifo
    ///     let pipe = Receiver::open("path/to/a/fifo")?;
    ///
    ///     loop {
    ///         // Wait for the pipe to be readable
    ///         pipe.readable().await?;
    ///
    ///         // Creating the buffer **after** the `await` prevents it from
    ///         // being stored in the async task.
    ///         let mut buf_a = [0; 512];
    ///         let mut buf_b = [0; 1024];
    ///         let mut bufs = [
    ///             IoSliceMut::new(&mut buf_a),
    ///             IoSliceMut::new(&mut buf_b),
    ///         ];
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match pipe.try_read_vectored(&mut bufs) {
    ///             Ok(0) => break,
    ///             Ok(n) => {
    ///                 println!("read {} bytes", n);
    ///             }
    ///             Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::READABLE, || (&*self.io).read_vectored(bufs))
    }
}

impl AsyncRead for Receiver {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Safety: `mio_pipe::Receiver` uses a `std::fs::File` underneath,
        // which correctly handles reads into uninitialized memory.
        unsafe { self.io.poll_read(cx, buf) }
    }
}

impl AsRawFd for Receiver {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

cfg_process! {
    use crate::process::{ChildStderr, ChildStdout};

    impl TryFrom<ChildStdout> for Receiver {
        type Error = io::Error;
        fn try_from(stdout: ChildStdout) -> io::Result<Receiver> {
            // Safety: ChildStdout has a valid fd to the reading end of a pipe.
            let mio_rx = unsafe { mio_pipe::Receiver::from_raw_fd(stdout.into_fd()?) };
            Receiver::from_mio(mio_rx)
        }
    }

    impl TryFrom<ChildStderr> for Receiver {
        type Error = io::Error;
        fn try_from(stderr: ChildStderr) -> io::Result<Receiver> {
            // Safety: ChildStderr has a valid fd to the reading end of a pipe.
            let mio_rx = unsafe { mio_pipe::Receiver::from_raw_fd(stderr.into_fd()?) };
            Receiver::from_mio(mio_rx)
        }
    }
}
