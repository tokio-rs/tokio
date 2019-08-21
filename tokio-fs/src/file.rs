//! Types for working with [`File`].
//!
//! [`File`]: file/struct.File.html

use crate::{asyncify, blocking_io, OpenOptions};

use tokio_io::{AsyncRead, AsyncWrite};

use std::convert::TryFrom;
use std::fs::{Metadata, Permissions};
use std::io::{self, Read, Seek, Write};
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
/// it was opened with. Files also implement Seek to alter the logical cursor
/// that the file contains internally.
///
/// Files are automatically closed when they go out of scope.
///
/// [std]: https://doc.rust-lang.org/std/fs/struct.File.html
///
/// # Examples
///
/// Create a new file and asynchronously write bytes to it:
///
/// ```no_run
/// use tokio::fs::File;
/// use tokio::prelude::*;
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
/// use tokio::prelude::*;
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
#[derive(Debug)]
pub struct File {
    std: std::fs::File,
}

impl File {
    /// Attempts to open a file in read-only mode.
    ///
    /// See [`OpenOptions`] for more details.
    ///
    /// [`OpenOptions`]: struct.OpenOptions.html
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
    /// use tokio::prelude::*;
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
    pub async fn open<P>(path: P) -> io::Result<File>
    where
        P: AsRef<Path> + Send + Unpin + 'static,
    {
        let mut open_options = OpenOptions::new();
        open_options.read(true);

        open_options.open(path).await
    }

    /// Opens a file in write-only mode.
    ///
    /// This function will create a file if it does not exist, and will truncate
    /// it if it does.
    ///
    /// See [`OpenOptions`] for more details.
    ///
    /// [`OpenOptions`]: struct.OpenOptions.html
    ///
    /// # Errors
    ///
    /// Results in an error if called from outside of the Tokio runtime or if
    /// the underlying [`create`] call results in an error.
    ///
    /// [`create`]: https://doc.rust-lang.org/std/fs/struct.File.html#method.create
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create<P>(path: P) -> io::Result<File>
    where
        P: AsRef<Path> + Send + Unpin + 'static,
    {
        let std_file = asyncify(|| std::fs::File::create(&path)).await?;
        Ok(File::from_std(std_file))
    }

    /// Convert a [`std::fs::File`][std] to a [`tokio_fs::File`][file].
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.File.html
    /// [file]: struct.File.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // This line could block. It is not recommended to do this on the Tokio
    /// // runtime.
    /// let std_file = std::fs::File::open("foo.txt").unwrap();
    /// let file = tokio::fs::File::from_std(std_file);
    /// ```
    pub fn from_std(std: std::fs::File) -> File {
        File { std }
    }

    /// Seek to an offset, in bytes, in a stream.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::prelude::*;
    ///
    /// use std::io::SeekFrom;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::open("foo.txt").await?;
    /// file.seek(SeekFrom::Start(6)).await?;
    ///
    /// let mut contents = vec![0u8; 10];
    /// file.read_exact(&mut contents).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        asyncify(|| self.std.seek(pos)).await
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
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// file.sync_all().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sync_all(&mut self) -> io::Result<()> {
        asyncify(|| self.std.sync_all()).await
    }

    /// This function is similar to `poll_sync_all`, except that it may not
    /// synchronize file metadata to the filesystem.
    ///
    /// This is intended for use cases that must synchronize content, but don't
    /// need the metadata on disk. The goal of this method is to reduce disk
    /// operations.
    ///
    /// Note that some platforms may simply implement this in terms of `poll_sync_all`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// file.sync_data().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sync_data(&mut self) -> io::Result<()> {
        asyncify(|| self.std.sync_data()).await
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
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::create("foo.txt").await?;
    /// file.write_all(b"hello, world!").await?;
    /// file.set_len(10).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_len(&mut self, size: u64) -> io::Result<()> {
        asyncify(|| self.std.set_len(size)).await
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
        asyncify(|| self.std.metadata()).await
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
        let std_file = asyncify(|| self.std.try_clone()).await?;
        Ok(File::from_std(std_file))
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
        asyncify(|| self.std.set_permissions(perm)).await
    }

    /// Destructures the `tokio_fs::File` into a [`std::fs::File`][std].
    ///
    /// # Panics
    ///
    /// This function will panic if `shutdown` has been called.
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.File.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let file = File::create("foo.txt").await?;
    /// let std_file = file.into_std();
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_std(self) -> std::fs::File {
        self.std
    }
}

impl AsyncRead for File {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        blocking_io(|| (&self.std).read(buf))
    }
}

impl AsyncWrite for File {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        blocking_io(|| (&self.std).write(buf))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        blocking_io(|| (&self.std).flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl From<std::fs::File> for File {
    fn from(std: std::fs::File) -> Self {
        Self::from_std(std)
    }
}

impl TryFrom<File> for std::fs::File {
    type Error = io::Error;

    fn try_from(file: File) -> Result<Self, Self::Error> {
        Ok(file.std)
    }
}
