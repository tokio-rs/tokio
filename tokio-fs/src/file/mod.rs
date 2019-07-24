//! Types for working with [`File`].
//!
//! [`File`]: file/struct.File.html

mod clone;
mod create;
mod metadata;
mod open;
mod open_options;
mod seek;

pub use self::clone::CloneFuture;
pub use self::create::CreateFuture;
pub use self::metadata::MetadataFuture;
pub use self::open::OpenFuture;
pub use self::open_options::OpenOptions;
pub use self::seek::SeekFuture;

use tokio_io::{AsyncRead, AsyncWrite};

use std::convert::TryFrom;
use std::fs::{File as StdFile, Metadata, Permissions};
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
/// use tokio::prelude::{AsyncWrite, Future};
///
/// let task = tokio::fs::File::create("foo.txt")
///     .and_then(|mut file| file.poll_write(b"hello, world!"))
///     .map(|res| {
///         println!("{:?}", res);
///     }).map_err(|err| eprintln!("IO error: {:?}", err));
///
/// tokio::run(task);
/// ```
///
/// Read the contents of a file into a buffer
///
/// ```no_run
/// use tokio::prelude::{AsyncRead, Future};
///
/// let task = tokio::fs::File::open("foo.txt")
///     .and_then(|mut file| {
///         let mut contents = vec![];
///         file.read_buf(&mut contents)
///             .map(|res| {
///                 println!("{:?}", res);
///             })
///     }).map_err(|err| eprintln!("IO error: {:?}", err));
///
/// tokio::run(task);
/// ```
#[derive(Debug)]
pub struct File {
    std: Option<StdFile>,
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
    /// `OpenFuture` results in an error if called from outside of the Tokio
    /// runtime or if the underlying [`open`] call results in an error.
    ///
    /// [`open`]: https://doc.rust-lang.org/std/fs/struct.File.html#method.open
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::open("foo.txt").and_then(|file| {
    ///     // do something with the file ...
    ///     file.metadata().map(|md| println!("{:?}", md))
    /// }).map_err(|e| {
    ///     // handle errors
    ///     eprintln!("IO error: {:?}", e);
    /// });
    ///
    /// tokio::run(task);
    /// ```
    pub fn open<P>(path: P) -> OpenFuture<P>
    where
        P: AsRef<Path> + Send + Unpin + 'static,
    {
        OpenOptions::new().read(true).open(path)
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
    /// `CreateFuture` results in an error if called from outside of the Tokio
    /// runtime or if the underlying [`create`] call results in an error.
    ///
    /// [`create`]: https://doc.rust-lang.org/std/fs/struct.File.html#method.create
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|file| {
    ///         // do something with the created file ...
    ///         file.metadata().map(|md| println!("{:?}", md))
    ///     }).map_err(|e| {
    ///         // handle errors
    ///         eprintln!("IO error: {:?}", e);
    /// });
    ///
    /// tokio::run(task);
    /// ```
    pub fn create<P>(path: P) -> CreateFuture<P>
    where
        P: AsRef<Path> + Send + Unpin + 'static,
    {
        CreateFuture::new(path)
    }

    /// Convert a [`std::fs::File`][std] to a [`tokio_fs::File`][file].
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.File.html
    /// [file]: struct.File.html
    ///
    /// Examples
    /// ```no_run
    /// use std::fs::File;
    ///
    /// let std_file = File::open("foo.txt").unwrap();
    /// let file = tokio::fs::File::from_std(std_file);
    /// ```
    pub fn from_std(std: StdFile) -> File {
        File { std: Some(std) }
    }

    /// Seek to an offset, in bytes, in a stream.
    ///
    /// A seek beyond the end of a stream is allowed, but implementation
    /// defined.
    ///
    /// If the seek operation completed successfully, this method returns the
    /// new position from the start of the stream. That position can be used
    /// later with `SeekFrom::Start`.
    ///
    /// # Errors
    ///
    /// Seeking to a negative offset is considered an error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    /// use std::io::SeekFrom;
    ///
    /// let task = tokio::fs::File::open("foo.txt")
    ///     // move cursor 6 bytes from the start of the file
    ///     .and_then(|mut file| file.poll_seek(SeekFrom::Start(6)))
    ///     .map(|res| {
    ///         println!("{:?}", res);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_seek(&mut self, pos: io::SeekFrom) -> Poll<io::Result<u64>> {
        crate::blocking_io(|| self.std().seek(pos))
    }

    /// Seek to an offset, in bytes, in a stream.
    ///
    /// Similar to `poll_seek`, but returning a `Future`.
    ///
    /// This method consumes the `File` and returns it back when the future
    /// completes.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    /// use std::io::SeekFrom;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|file| file.seek(SeekFrom::Start(6)))
    ///     .map(|file| {
    ///         // handle returned file ..
    ///         # println!("{:?}", file);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn seek(self, pos: io::SeekFrom) -> SeekFuture {
        SeekFuture::new(self, pos)
    }

    /// Attempts to sync all OS-internal metadata to disk.
    ///
    /// This function will attempt to ensure that all in-core data reaches the
    /// filesystem before returning.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::{AsyncWrite, Future};
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|mut file| {
    ///         file.poll_write(b"hello, world!")?;
    ///         file.poll_sync_all()
    ///     })
    ///     .map(|res| {
    ///         // handle returned result ..
    ///         # println!("{:?}", res);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_sync_all(&mut self) -> Poll<io::Result<()>> {
        crate::blocking_io(|| self.std().sync_all())
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
    /// use tokio::prelude::{AsyncWrite, Future};
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|mut file| {
    ///         file.poll_write(b"hello, world!")?;
    ///         file.poll_sync_data()
    ///     })
    ///     .map(|res| {
    ///         // handle returned result ..
    ///         # println!("{:?}", res);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_sync_data(&mut self) -> Poll<io::Result<()>> {
        crate::blocking_io(|| self.std().sync_data())
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
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|mut file| {
    ///         file.poll_set_len(10)
    ///     })
    ///     .map(|res| {
    ///         // handle returned result ..
    ///         # println!("{:?}", res);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_set_len(&mut self, size: u64) -> Poll<io::Result<()>> {
        crate::blocking_io(|| self.std().set_len(size))
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|file| file.metadata())
    ///     .map(|metadata| {
    ///         println!("{:?}", metadata);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn metadata(self) -> MetadataFuture {
        MetadataFuture::new(self)
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|mut file| file.poll_metadata())
    ///     .map(|metadata| {
    ///         // metadata is of type Async::Ready<Metadata>
    ///         println!("{:?}", metadata);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_metadata(&mut self) -> Poll<io::Result<Metadata>> {
        crate::blocking_io(|| self.std().metadata())
    }

    /// Create a new `File` instance that shares the same underlying file handle
    /// as the existing `File` instance. Reads, writes, and seeks will affect both
    /// File instances simultaneously.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|mut file| file.poll_try_clone())
    ///     .map(|clone| {
    ///         // do something with the clone
    ///         # println!("{:?}", clone);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_try_clone(&mut self) -> Poll<io::Result<File>> {
        crate::blocking_io(|| {
            let std = self.std().try_clone()?;
            Ok(File::from_std(std))
        })
    }

    /// Create a new `File` instance that shares the same underlying file handle
    /// as the existing `File` instance. Reads, writes, and seeks will affect both
    /// File instances simultaneously.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|file| {
    ///         file.try_clone()
    ///             .map(|(file, clone)| {
    ///                 // do something with the file and the clone
    ///                 # println!("{:?} {:?}", file, clone);
    ///             })
    ///             .map_err(|(file, err)| {
    ///                 // you get the original file back if there's an error
    ///                 # println!("{:?}", file);
    ///                 err
    ///             })
    ///     })
    ///     .map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn try_clone(self) -> CloneFuture {
        CloneFuture::new(self)
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
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .and_then(|file| file.metadata())
    ///     .map(|(mut file, metadata)| {
    ///         let mut perms = metadata.permissions();
    ///         perms.set_readonly(true);
    ///         match file.poll_set_permissions(perms) {
    ///             Err(e) => eprintln!("{}", e),
    ///             _ => println!("permissions set!"),
    ///         }
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn poll_set_permissions(&mut self, perm: Permissions) -> Poll<io::Result<()>> {
        crate::blocking_io(|| self.std().set_permissions(perm))
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
    /// use tokio::prelude::Future;
    ///
    /// let task = tokio::fs::File::create("foo.txt")
    ///     .map(|file| {
    ///         let std_file = file.into_std();
    ///         // do something with the std::fs::File
    ///         # println!("{:?}", std_file);
    ///     }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    /// tokio::run(task);
    /// ```
    pub fn into_std(mut self) -> StdFile {
        self.std.take().expect("`File` instance already shutdown")
    }

    fn std(&mut self) -> &mut StdFile {
        self.std.as_mut().expect("`File` instance already shutdown")
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        crate::would_block(|| self.std().read(buf))
    }
}

impl AsyncRead for File {
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

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        crate::would_block(|| self.std().write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        crate::would_block(|| self.std().flush())
    }
}

impl AsyncWrite for File {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::get_mut(self).write(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            other => Poll::Ready(other),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match Pin::get_mut(self).flush() {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            other => Poll::Ready(other),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if let Some(_std) = self.std.take() {
            // This is probably fine as closing a file *shouldn't* be a blocking
            // operation. That said, ideally `shutdown` is called first.
        }
    }
}

impl From<StdFile> for File {
    fn from(std: StdFile) -> Self {
        Self::from_std(std)
    }
}

impl TryFrom<File> for StdFile {
    type Error = io::Error;

    fn try_from(mut file: File) -> Result<Self, Self::Error> {
        file.std
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "`File` instance already shutdown"))
    }
}
