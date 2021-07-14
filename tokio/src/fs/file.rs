//! Types for working with [`File`].
//!
//! [`File`]: File

use crate::fs::{asyncify, sys};
use crate::io::{AsyncRead, AsyncSeek, AsyncWrite, Asyncify, ReadBuf};

use std::fmt;
use std::fs::{Metadata, Permissions};
use std::io::{self, Seek, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// A reference to an open file on the filesystem.
///
/// This is a specialized version of [`std::fs::File`][std] for usage from the
/// Tokio runtime.
///
/// An instance of a `File` can be read and/or written depending on what options
/// it was opened with. Files also implement [`AsyncSeek`] to alter the logical
/// cursor that the file contains internally.
///
/// A file will not be closed immediately when it goes out of scope if there
/// are any IO operations that have not yet completed. To ensure that a file is
/// closed immediately when it is dropped, you should call [`flush`] before
/// dropping it. Note that this does not ensure that the file has been fully
/// written to disk; the operating system might keep the changes around in an
/// in-memory buffer. See the [`sync_all`] method for telling the OS to write
/// the data to disk.
///
/// Reading and writing to a `File` is usually done using the convenience
/// methods found on the [`AsyncReadExt`] and [`AsyncWriteExt`] traits.
///
/// [std]: struct@std::fs::File
/// [`AsyncSeek`]: trait@crate::io::AsyncSeek
/// [`flush`]: fn@crate::io::AsyncWriteExt::flush
/// [`sync_all`]: fn@crate::fs::File::sync_all
/// [`AsyncReadExt`]: trait@crate::io::AsyncReadExt
/// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
///
/// # Examples
///
/// Create a new file and asynchronously write bytes to it:
///
/// ```no_run
/// use tokio::fs::File;
/// use tokio::io::AsyncWriteExt; // for write_all()
///
/// # async fn dox() -> std::io::Result<()> {
/// let mut file = File::create("foo.txt").await?;
/// file.write_all(b"hello, world!").await?;
/// # Ok(())
/// # }
/// ```
///
/// Read the contents of a file into a buffer
///
/// ```no_run
/// use tokio::fs::File;
/// use tokio::io::AsyncReadExt; // for read_to_end()
///
/// # async fn dox() -> std::io::Result<()> {
/// let mut file = File::open("foo.txt").await?;
///
/// let mut contents = vec![];
/// file.read_to_end(&mut contents).await?;
///
/// println!("len = {}", contents.len());
/// # Ok(())
/// # }
/// ```
pub struct File {
    inner: Asyncify<sys::File>,
}

impl File {
    /// Attempts to open a file in read-only mode.
    ///
    /// See [`OpenOptions`] for more details.
    ///
    /// [`OpenOptions`]: super::OpenOptions
    ///
    /// # Errors
    ///
    /// This function will return an error if called from outside of the Tokio
    /// runtime or if path does not already exist. Other errors may also be
    /// returned according to OpenOptions::open.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::io::AsyncReadExt;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::open("foo.txt").await?;
    ///
    /// let mut contents = vec![];
    /// file.read_to_end(&mut contents).await?;
    ///
    /// println!("len = {}", contents.len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The [`read_to_end`] method is defined on the [`AsyncReadExt`] trait.
    ///
    /// [`read_to_end`]: fn@crate::io::AsyncReadExt::read_to_end
    /// [`AsyncReadExt`]: trait@crate::io::AsyncReadExt
    pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let std = asyncify(|| sys::File::open(path)).await?;

        Ok(File::from_std(std))
    }

    /// Opens a file in write-only mode.
    ///
    /// This function will create a file if it does not exist, and will truncate
    /// it if it does.
    ///
    /// See [`OpenOptions`] for more details.
    ///
    /// [`OpenOptions`]: super::OpenOptions
    ///
    /// # Errors
    ///
    /// Results in an error if called from outside of the Tokio runtime or if
    /// the underlying [`create`] call results in an error.
    ///
    /// [`create`]: std::fs::File::create
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The [`write_all`] method is defined on the [`AsyncWriteExt`] trait.
    ///
    /// [`write_all`]: fn@crate::io::AsyncWriteExt::write_all
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let std_file = asyncify(move || sys::File::create(path)).await?;
        Ok(File::from_std(std_file))
    }

    /// Converts a [`std::fs::File`][std] to a [`tokio::fs::File`][file].
    ///
    /// [std]: std::fs::File
    /// [file]: File
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // This line could block. It is not recommended to do this on the Tokio
    /// // runtime.
    /// let std_file = std::fs::File::open("foo.txt").unwrap();
    /// let file = tokio::fs::File::from_std(std_file);
    /// ```
    pub fn from_std(std: sys::File) -> File {
        File {
            inner: Asyncify::new(std),
        }
    }

    /// Attempts to sync all OS-internal metadata to disk.
    ///
    /// This function will attempt to ensure that all in-core data reaches the
    /// filesystem before returning.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// file.sync_all().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The [`write_all`] method is defined on the [`AsyncWriteExt`] trait.
    ///
    /// [`write_all`]: fn@crate::io::AsyncWriteExt::write_all
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    pub async fn sync_all(&self) -> io::Result<()> {
        self.inner.complete_inflight().await;
        self.inner.asyncify(|file| file.sync_all()).await
    }

    /// This function is similar to `sync_all`, except that it may not
    /// synchronize file metadata to the filesystem.
    ///
    /// This is intended for use cases that must synchronize content, but don't
    /// need the metadata on disk. The goal of this method is to reduce disk
    /// operations.
    ///
    /// Note that some platforms may simply implement this in terms of `sync_all`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// file.sync_data().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The [`write_all`] method is defined on the [`AsyncWriteExt`] trait.
    ///
    /// [`write_all`]: fn@crate::io::AsyncWriteExt::write_all
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    pub async fn sync_data(&self) -> io::Result<()> {
        self.inner.complete_inflight().await;
        self.inner.asyncify(|file| file.sync_data()).await
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    ///
    /// If the size is less than the current file's size, then the file will be
    /// shrunk. If it is greater than the current file's size, then the file
    /// will be extended to size and have all of the intermediate data filled in
    /// with 0s.
    ///
    /// # Errors
    ///
    /// This function will return an error if the file is not opened for
    /// writing.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::io::AsyncWriteExt;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// file.set_len(10).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The [`write_all`] method is defined on the [`AsyncWriteExt`] trait.
    ///
    /// [`write_all`]: fn@crate::io::AsyncWriteExt::write_all
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    pub async fn set_len(&self, size: u64) -> io::Result<()> {
        use crate::io::asyncify::{Operation, State};

        let mut inner = self.inner.inner.lock().await;
        inner.complete_inflight().await;

        let mut buf = match inner.state {
            State::Idle(ref mut buf_cell) => buf_cell.take().unwrap(),
            _ => unreachable!(),
        };

        let seek = if !buf.is_empty() {
            Some(SeekFrom::Current(buf.discard_read()))
        } else {
            None
        };

        let std = self.inner.io.clone();

        inner.state = State::Busy(sys::run(move || {
            let res = if let Some(seek) = seek {
                (&*std).seek(seek).and_then(|_| std.set_len(size))
            } else {
                std.set_len(size)
            }
            .map(|_| 0); // the value is discarded later

            // Return the result as a seek
            (Operation::Seek(res), buf)
        }));

        let (op, buf) = match inner.state {
            State::Idle(_) => unreachable!(),
            State::Busy(ref mut rx) => rx.await?,
        };

        inner.state = State::Idle(Some(buf));

        match op {
            Operation::Seek(res) => res.map(|pos| {
                inner.pos = pos;
            }),
            _ => unreachable!(),
        }
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let file = File::open("foo.txt").await?;
    /// let metadata = file.metadata().await?;
    ///
    /// println!("{:?}", metadata);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn metadata(&self) -> io::Result<Metadata> {
        self.inner.asyncify(|file| file.metadata()).await
    }

    /// Create a new `File` instance that shares the same underlying file handle
    /// as the existing `File` instance. Reads, writes, and seeks will affect both
    /// File instances simultaneously.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let file = File::open("foo.txt").await?;
    /// let file_clone = file.try_clone().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn try_clone(&self) -> io::Result<File> {
        let std_file = self.inner.asyncify(|file| file.try_clone()).await?;
        Ok(File::from_std(std_file))
    }

    /// Destructures `File` into a [`std::fs::File`][std]. This function is
    /// async to allow any in-flight operations to complete.
    ///
    /// Use `File::try_into_std` to attempt conversion immediately.
    ///
    /// [std]: std::fs::File
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let tokio_file = File::open("foo.txt").await?;
    /// let std_file = tokio_file.into_std().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn into_std(self) -> sys::File {
        self.inner.into_inner().await
    }

    /// Tries to immediately destructure `File` into a [`std::fs::File`][std].
    ///
    /// [std]: std::fs::File
    ///
    /// # Errors
    ///
    /// This function will return an error containing the file if some
    /// operation is in-flight.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let tokio_file = File::open("foo.txt").await?;
    /// let std_file = tokio_file.try_into_std().unwrap();
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_into_std(self) -> Result<sys::File, Self> {
        self.inner
            .try_into_inner()
            .map_err(|asyncify| Self { inner: asyncify })
    }

    /// Changes the permissions on the underlying file.
    ///
    /// # Platform-specific behavior
    ///
    /// This function currently corresponds to the `fchmod` function on Unix and
    /// the `SetFileInformationByHandle` function on Windows. Note that, this
    /// [may change in the future][changes].
    ///
    /// [changes]: https://doc.rust-lang.org/std/io/index.html#platform-specific-behavior
    ///
    /// # Errors
    ///
    /// This function will return an error if the user lacks permission change
    /// attributes on the underlying file. It may also return an error in other
    /// os-specific unspecified cases.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let file = File::open("foo.txt").await?;
    /// let mut perms = file.metadata().await?.permissions();
    /// perms.set_readonly(true);
    /// file.set_permissions(perms).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_permissions(&self, perm: Permissions) -> io::Result<()> {
        self.inner.asyncify(|file| file.set_permissions(perm)).await
    }
}

impl AsyncRead for File {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        dst: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, dst)
    }
}

impl AsyncSeek for File {
    fn start_seek(mut self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
        Pin::new(&mut self.inner).start_seek(pos)
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Pin::new(&mut self.inner).poll_complete(cx)
    }
}

impl AsyncWrite for File {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        src: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, src)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl From<sys::File> for File {
    fn from(std: sys::File) -> Self {
        Self::from_std(std)
    }
}

impl fmt::Debug for File {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("tokio::fs::File")
            .field("std", &self.inner.io)
            .finish()
    }
}

#[cfg(unix)]
impl std::os::unix::io::AsRawFd for File {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.inner.io.as_raw_fd()
    }
}

#[cfg(unix)]
impl std::os::unix::io::FromRawFd for File {
    unsafe fn from_raw_fd(fd: std::os::unix::io::RawFd) -> Self {
        sys::File::from_raw_fd(fd).into()
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for File {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.inner.io.as_raw_handle()
    }
}

#[cfg(windows)]
impl std::os::windows::io::FromRawHandle for File {
    unsafe fn from_raw_handle(handle: std::os::windows::io::RawHandle) -> Self {
        sys::File::from_raw_handle(handle).into()
    }
}
