//! Types for working with [`File`].
//!
//! [`File`]: file/struct.File.html

mod create;
mod metadata;
mod open;
mod open_options;
mod seek;

pub use self::create::CreateFuture;
pub use self::metadata::MetadataFuture;
pub use self::open::OpenFuture;
pub use self::open_options::OpenOptions;
pub use self::seek::SeekFuture;

use tokio_io::{AsyncRead, AsyncWrite};

use futures::Poll;

use std::fs::{File as StdFile, Metadata, Permissions};
use std::io::{self, Read, Write, Seek};
use std::path::Path;

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
    pub fn open<P>(path: P) -> OpenFuture<P>
    where P: AsRef<Path> + Send + 'static,
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
    pub fn create<P>(path: P) -> CreateFuture<P>
    where P: AsRef<Path> + Send + 'static,
    {
        CreateFuture::new(path)
    }

    /// Convert a [`std::fs::File`][std] to a `tokio_fs::File`.
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.File.html
    pub(crate) fn from_std(std: StdFile) -> File {
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
    pub fn poll_seek(&mut self, pos: io::SeekFrom) -> Poll<u64, io::Error> {
        ::blocking_io(|| self.std().seek(pos))
    }

    /// Seek to an offset, in bytes, in a stream.
    ///
    /// Similar to `poll_seek`, but returning a `Future`.
    ///
    /// This method consumes the `File` and returns it back when the future
    /// completes.
    pub fn seek(self, pos: io::SeekFrom) -> SeekFuture {
        SeekFuture::new(self, pos)
    }

    /// Attempts to sync all OS-internal metadata to disk.
    ///
    /// This function will attempt to ensure that all in-core data reaches the
    /// filesystem before returning.
    pub fn poll_sync_all(&mut self) -> Poll<(), io::Error> {
        ::blocking_io(|| self.std().sync_all())
    }

    /// This function is similar to `poll_sync_all`, except that it may not
    /// synchronize file metadata to the filesystem.
    ///
    /// This is intended for use cases that must synchronize content, but don't
    /// need the metadata on disk. The goal of this method is to reduce disk
    /// operations.
    ///
    /// Note that some platforms may simply implement this in terms of `poll_sync_all`.
    pub fn poll_sync_data(&mut self) -> Poll<(), io::Error> {
        ::blocking_io(|| self.std().sync_data())
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
    pub fn poll_set_len(&mut self, size: u64) -> Poll<(), io::Error> {
        ::blocking_io(|| self.std().set_len(size))
    }

    /// Queries metadata about the underlying file.
    pub fn metadata(self) -> MetadataFuture {
        MetadataFuture::new(self)
    }

    /// Queries metadata about the underlying file.
    pub fn poll_metadata(&mut self) -> Poll<Metadata, io::Error> {
        ::blocking_io(|| self.std().metadata())
    }

    /// Create a new `File` instance that shares the same underlying file handle
    /// as the existing `File` instance. Reads, writes, and seeks will affect both
    /// File instances simultaneously.
    pub fn poll_try_clone(&mut self) -> Poll<File, io::Error> {
        ::blocking_io(|| {
            let std = self.std().try_clone()?;
            Ok(File::from_std(std))
        })
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
    pub fn poll_set_permissions(&mut self, perm: Permissions) -> Poll<(), io::Error> {
        ::blocking_io(|| self.std().set_permissions(perm))
    }

    /// Destructures the `tokio_fs::File` into a [`std::fs::File`][std].
    ///
    /// # Panics
    ///
    /// This function will panic if `shutdown` has been called.
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.File.html
    pub fn into_std(mut self) -> StdFile {
        self.std.take().expect("`File` instance already shutdown")
    }

    fn std(&mut self) -> &mut StdFile {
        self.std.as_mut().expect("`File` instance already shutdown")
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        ::would_block(|| self.std().read(buf))
    }
}

impl AsyncRead for File {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        ::would_block(|| self.std().write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        ::would_block(|| self.std().flush())
    }
}

impl AsyncWrite for File {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        ::blocking_io(|| {
            self.std = None;
            Ok(())
        })
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
