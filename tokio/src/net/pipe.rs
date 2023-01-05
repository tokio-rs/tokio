//! Unix pipe types.

use crate::io::interest::Interest;
use crate::io::{AsyncRead, AsyncWrite, PollEvented, ReadBuf, Ready};

use mio::unix::pipe as mio_pipe;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::prelude::OpenOptionsExt;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    use bytes::BufMut;
}

cfg_net_unix! {
    /// Writing end of a Unix pipe.
    #[derive(Debug)]
    pub struct Sender {
        io: PollEvented<mio_pipe::Sender>,
    }
}

impl Sender {
    /// Creates a new `Sender` from a FIFO file.
    ///
    /// This function will open the FIFO file at the specified path and associate
    /// the pipe with the default event loop for writing.
    ///
    /// This function will fail if no one has this pipe open for reading. You can
    /// wait for a reader using sleeping in a loop. On Linux you can also use
    /// [`open_rw`] to work around this.
    ///
    /// [`open_rw`]: Self::open_rw
    ///
    /// # Errors
    ///
    /// If the FIFO file is not open for reading this function will fail with
    /// `ENXIO`. This function may also fail with other standard OS errors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe::Sender;
    /// use tokio::time::{self, Duration};
    /// use nix::{unistd::mkfifo, sys::stat::Mode};
    /// # use std::error::Error;
    ///
    /// const FIFO_NAME: &str = "path/to/a/new/fifo";
    ///
    /// # async fn dox() -> Result<(), Box<dyn Error>> {
    /// // Create a new FIFO file.
    /// mkfifo(FIFO_NAME, Mode::S_IRWXU)?;
    ///
    /// // Wait for a reader to open the file.
    /// let tx = loop {
    ///     match Sender::open(FIFO_NAME) {
    ///         Ok(tx) => break tx,
    ///         Err(e) if e.raw_os_error() == Some(libc::ENXIO) => {},
    ///         Err(e) => return Err(e.into()),
    ///     }
    ///
    ///     time::sleep(Duration::from_millis(50)).await;
    /// };
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if it is not called from within a runtime with
    /// IO enabled.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn open<P>(path: P) -> io::Result<Sender>
    where
        P: AsRef<Path>,
    {
        Sender::open_internal(path.as_ref(), false)
    }

    /// Creates a new `Sender` from a FIFO file without a present reader.
    ///
    /// This function will open the FIFO file at the specified path and associate
    /// the pipe with the default event loop for writing.
    ///
    /// Unlike [`open`], this function will not fail if no one has this pipe open
    /// for reading. This is done by opening the FIFO file with access to both
    /// reading and writing. Note that the behavior of such operation is not defined
    /// by the POSIX standard and is only guaranteed to work on Linux.
    ///
    /// [`open`]: Self::open
    ///
    /// # Errors
    ///
    /// Fails with any standard OS error if it occurs.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::io::AsyncWriteExt;
    /// use tokio::net::pipe::Sender;
    /// use nix::{unistd::mkfifo, sys::stat::Mode};
    /// # use std::error::Error;
    ///
    /// const FIFO_NAME: &str = "path/to/a/new/fifo";
    ///
    /// # async fn dox() -> Result<(), Box<dyn Error>> {
    /// // Create a new FIFO file.
    /// mkfifo(FIFO_NAME, Mode::S_IRWXU)?;
    ///
    /// // `Sender::open` would fail here, since there is no open reading end.
    /// let mut tx = Sender::open_rw(FIFO_NAME)?;
    /// // We can asynchronously write to the pipe before any readers.
    /// tx.write_all(b"hello world").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if it is not called from within a runtime with
    /// IO enabled.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn open_rw<P>(path: P) -> io::Result<Sender>
    where
        P: AsRef<Path>,
    {
        Sender::open_internal(path.as_ref(), true)
    }

    fn open_internal(path: &Path, read_access: bool) -> io::Result<Sender> {
        let file = std::fs::OpenOptions::new()
            .read(read_access)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        let raw_fd = file.into_raw_fd();
        // Safety: Raw fd was created a valid file.
        let mio_tx = unsafe { mio_pipe::Sender::from_raw_fd(raw_fd) };
        Sender::from_mio(mio_tx)
    }

    fn from_mio(mio_tx: mio_pipe::Sender) -> io::Result<Sender> {
        let io = PollEvented::new_with_interest(mio_tx, Interest::WRITABLE)?;
        Ok(Sender { io })
    }

    /// Creates a new `Sender` from a [`File`].
    ///
    /// This function is intended to construct a pipe from a File which represents
    /// a special FIFO file. The conversion assumes nothing about the underlying
    /// file; it is left up to the user to open it with writing access and set it
    /// in non-blocking mode.
    ///
    /// You can check first if a [`File`] has the FIFO file type and then use this
    /// function to get a `Sender`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe;
    /// use std::fs::OpenOptions;
    /// use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
    /// # use std::error::Error;
    ///
    /// const FIFO_NAME: &str = "path/to/a/fifo";
    ///
    /// # async fn dox() -> Result<(), Box<dyn Error>> {
    /// let file = OpenOptions::new()
    ///     .write(true)
    ///     .custom_flags(libc::O_NONBLOCK)
    ///     .open(FIFO_NAME)?;
    /// if file.metadata()?.file_type().is_fifo() {
    ///     let tx = pipe::Sender::from_file(file)?;
    ///     /* use the Sender */
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if it is not called from within a runtime with
    /// IO enabled.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn from_file(file: File) -> io::Result<Sender> {
        let raw_fd = file.into_raw_fd();
        let mio_tx = unsafe { mio_pipe::Sender::from_raw_fd(raw_fd) };
        Sender::from_mio(mio_tx)
    }

    /// Waits for any of the requested ready states.
    ///
    /// This function can be used to wait for a [`WRITE_CLOSED`] event.
    ///
    /// The function may complete without the socket being ready. This is a
    /// false-positive and attempting an operation will return with
    /// `io::ErrorKind::WouldBlock`. The function can also return with an empty
    /// [`Ready`] set, so you should always check the returned value and possibly
    /// wait again if the requested states are not set.
    ///
    /// [`WRITE_CLOSED`]: Ready::WRITE_CLOSED
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. Once a readiness event occurs, the method
    /// will continue to return immediately until the readiness event is
    /// consumed by an attempt to read or write that fails with `WouldBlock` or
    /// `Poll::Pending`.
    pub async fn ready(&self, interest: Interest) -> io::Result<Ready> {
        let event = self.io.registration().readiness(interest).await?;
        Ok(event.ready)
    }

    /// Waits for the pipe to become writable.
    ///
    /// This function is equivalent to `ready(Interest::WRITABLE)` and is usually
    /// paired with [`try_write()`].
    ///
    /// [`try_write()`]: Self::try_write
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a writing end of a fifo
    ///     let tx = pipe::Sender::open("path/to/a/fifo")?;
    ///
    ///     loop {
    ///         // Wait for the pipe to be writable
    ///         tx.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match tx.try_write(b"hello world") {
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
        self.ready(Interest::WRITABLE).await?;
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
    /// use tokio::net::pipe;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a writing end of a fifo
    ///     let tx = pipe::Sender::open("path/to/a/fifo")?;
    ///
    ///     loop {
    ///         // Wait for the pipe to be writable
    ///         tx.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match tx.try_write(b"hello world") {
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
    /// use tokio::net::pipe;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a writing end of a fifo
    ///     let tx = pipe::Sender::open("path/to/a/fifo")?;
    ///
    ///     let bufs = [io::IoSlice::new(b"hello "), io::IoSlice::new(b"world")];
    ///
    ///     loop {
    ///         // Wait for the pipe to be writable
    ///         tx.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match tx.try_write_vectored(&bufs) {
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

cfg_net_unix! {
    /// Reading end of a Unix pipe.
    #[derive(Debug)]
    pub struct Receiver {
        io: PollEvented<mio_pipe::Receiver>,
    }
}

impl Receiver {
    /// Creates a new `Receiver` from a FIFO file.
    ///
    /// This function will open the FIFO file at the specified path and associate
    /// the pipe with the default event loop for reading.
    ///
    /// # Errors
    ///
    /// Fails with any standard OS error if it occurs.
    ///
    /// # Panics
    ///
    /// This function panics if it is not called from within a runtime with
    /// IO enabled.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn open<P>(path: P) -> io::Result<Receiver>
    where
        P: AsRef<Path>,
    {
        Receiver::open_internal(path.as_ref())
    }

    fn open_internal(path: &Path) -> io::Result<Receiver> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        let raw_fd = file.into_raw_fd();
        // Safety: Raw fd was created a valid file.
        let mio_rx = unsafe { mio_pipe::Receiver::from_raw_fd(raw_fd) };
        Receiver::from_mio(mio_rx)
    }

    fn from_mio(mio_rx: mio_pipe::Receiver) -> io::Result<Receiver> {
        let io = PollEvented::new_with_interest(mio_rx, Interest::READABLE)?;
        Ok(Receiver { io })
    }

    /// Creates new `Receiver` from a [`File`].
    ///
    /// This function is intended to create a pipe from a File which represents
    /// a special FIFO file. The conversion assumes nothing about the underlying
    /// file; it is left up to the user to open it with reading access and set it
    /// in non-blocking mode.
    ///
    /// You can check first if a [`File`] has the FIFO file type and then use this
    /// function to get a `Receiver`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe;
    /// use std::fs::OpenOptions;
    /// use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
    /// # use std::error::Error;
    ///
    /// const FIFO_NAME: &str = "path/to/a/fifo";
    ///
    /// # async fn dox() -> Result<(), Box<dyn Error>> {
    /// let file = OpenOptions::new()
    ///     .read(true)
    ///     .custom_flags(libc::O_NONBLOCK)
    ///     .open(FIFO_NAME)?;
    /// if file.metadata()?.file_type().is_fifo() {
    ///     let tx = pipe::Receiver::from_file(file)?;
    ///     /* use the Receiver */
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if it is not called from within a runtime with
    /// IO enabled.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn from_file(file: File) -> io::Result<Receiver> {
        let raw_fd = file.into_raw_fd();
        let mio_rx = unsafe { mio_pipe::Receiver::from_raw_fd(raw_fd) };
        Receiver::from_mio(mio_rx)
    }

    /// Waits for any of the requested ready states.
    ///
    /// This function can be used to wait for a [`READ_CLOSED`] event.
    ///
    /// The function may complete without the socket being ready. This is a
    /// false-positive and attempting an operation will return with
    /// `io::ErrorKind::WouldBlock`. The function can also return with an empty
    /// [`Ready`] set, so you should always check the returned value and possibly
    /// wait again if the requested states are not set.
    ///
    /// [`READ_CLOSED`]: Ready::READ_CLOSED
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. Once a readiness event occurs, the method
    /// will continue to return immediately until the readiness event is
    /// consumed by an attempt to read or write that fails with `WouldBlock` or
    /// `Poll::Pending`.
    pub async fn ready(&self, interest: Interest) -> io::Result<Ready> {
        let event = self.io.registration().readiness(interest).await?;
        Ok(event.ready)
    }

    /// Waits for the pipe to become readable.
    ///
    /// This function is equivalent to `ready(Interest::READABLE)` and is usually
    /// paired with [`try_read()`].
    ///
    /// [`try_read()`]: Self::try_read()
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::pipe;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a reading end of a fifo
    ///     let rx = pipe::Receiver::open("path/to/a/fifo")?;
    ///
    ///     let mut msg = vec![0; 1024];
    ///
    ///     loop {
    ///         // Wait for the pipe to be readable
    ///         rx.readable().await?;
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match rx.try_read(&mut msg) {
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
        self.ready(Interest::READABLE).await?;
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
    /// use tokio::net::pipe;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a reading end of a fifo
    ///     let rx = pipe::Receiver::open("path/to/a/fifo")?;
    ///
    ///     let mut msg = vec![0; 1024];
    ///
    ///     loop {
    ///         // Wait for the pipe to be readable
    ///         rx.readable().await?;
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match rx.try_read(&mut msg) {
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
    /// use tokio::net::pipe;
    /// use std::error::Error;
    /// use std::io::{self, IoSliceMut};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Open a reading end of a fifo
    ///     let rx = pipe::Receiver::open("path/to/a/fifo")?;
    ///
    ///     loop {
    ///         // Wait for the pipe to be readable
    ///         rx.readable().await?;
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
    ///         match rx.try_read_vectored(&mut bufs) {
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

    cfg_io_util! {
        /// Tries to read data from the stream into the provided buffer, advancing the
        /// buffer's internal cursor, returning how many bytes were read.
        ///
        /// Receives any pending data from the pipe but does not wait for new data
        /// to arrive. On success, returns the number of bytes read. Because
        /// `try_read_buf()` is non-blocking, the buffer does not have to be stored by
        /// the async task and can exist entirely on the stack.
        ///
        /// Usually, [`readable()`] or [`ready()`] is used with this function.
        ///
        /// [`readable()`]: Self::readable
        /// [`ready()`]: Self::ready
        ///
        /// # Return
        ///
        /// If data is successfully read, `Ok(n)` is returned, where `n` is the
        /// number of bytes read. `Ok(0)` indicates the stream's read half is closed
        /// and will no longer yield data. If the stream is not ready to read data
        /// `Err(io::ErrorKind::WouldBlock)` is returned.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tokio::net::pipe;
        /// use std::error::Error;
        /// use std::io;
        ///
        /// #[tokio::main]
        /// async fn main() -> Result<(), Box<dyn Error>> {
        ///     // Open a reading end of a fifo
        ///     let rx = pipe::Receiver::open("path/to/a/fifo")?;
        ///
        ///     loop {
        ///         // Wait for the pipe to be readable
        ///         rx.readable().await?;
        ///
        ///         let mut buf = Vec::with_capacity(4096);
        ///
        ///         // Try to read data, this may still fail with `WouldBlock`
        ///         // if the readiness event is a false positive.
        ///         match rx.try_read_buf(&mut buf) {
        ///             Ok(0) => break,
        ///             Ok(n) => {
        ///                 println!("read {} bytes", n);
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
        pub fn try_read_buf<B: BufMut>(&self, buf: &mut B) -> io::Result<usize> {
            self.io.registration().try_io(Interest::READABLE, || {
                use std::io::Read;

                let dst = buf.chunk_mut();
                let dst =
                    unsafe { &mut *(dst as *mut _ as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]) };

                // Safety: We trust `mio_pipe::Receiver::read` to have filled up `n` bytes in the
                // buffer.
                let n = (&*self.io).read(dst)?;

                unsafe {
                    buf.advance_mut(n);
                }

                Ok(n)
            })
        }
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
