use crate::{asyncify, blocking_io};

use futures_core::stream::Stream;
use std::ffi::OsString;
use std::fs::{DirEntry as StdDirEntry, FileType, Metadata};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::DirEntryExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Returns a stream over the entries within a directory.
///
/// This is an async version of [`std::fs::read_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.read_dir.html
pub async fn read_dir<P>(path: P) -> io::Result<ReadDir>
where
    P: AsRef<Path> + Send + 'static,
{
    let std = asyncify(|| std::fs::read_dir(&path)).await?;
    Ok(ReadDir(std))
}

/// Stream of the entries in a directory.
///
/// This stream is returned from the [`read_dir`] function of this module and
/// will yield instances of [`DirEntry`]. Through a [`DirEntry`]
/// information like the entry's path and possibly other metadata can be
/// learned.
///
/// # Errors
///
/// This [`Stream`] will return an [`Err`] if there's some sort of intermittent
/// IO error during iteration.
///
/// [`read_dir`]: fn.read_dir.html
/// [`DirEntry`]: struct.DirEntry.html
/// [`Stream`]: ../futures/stream/trait.Stream.html
/// [`Err`]: https://doc.rust-lang.org/std/result/enum.Result.html#variant.Err
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct ReadDir(std::fs::ReadDir);

impl Stream for ReadDir {
    type Item = io::Result<DirEntry>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let res = blocking_io(|| match self.0.next() {
            Some(Err(err)) => Err(err),
            Some(Ok(item)) => Ok(Some(Ok(DirEntry(item)))),
            None => Ok(None),
        });

        match res {
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err))),
            Poll::Ready(Ok(v)) => Poll::Ready(v),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Entries returned by the [`ReadDir`] stream.
///
/// [`ReadDir`]: struct.ReadDir.html
///
/// This is a specialized version of [`std::fs::DirEntry`][std] for usage from the
/// Tokio runtime.
///
/// An instance of `DirEntry` represents an entry inside of a directory on the
/// filesystem. Each entry can be inspected via methods to learn about the full
/// path or possibly other metadata through per-platform extension traits.
///
/// [std]: https://doc.rust-lang.org/std/fs/struct.DirEntry.html
#[derive(Debug)]
pub struct DirEntry(StdDirEntry);

impl DirEntry {
    /// Destructures the `tokio_fs::DirEntry` into a [`std::fs::DirEntry`][std].
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.DirEntry.html
    pub fn into_std(self) -> StdDirEntry {
        self.0
    }

    /// Returns the full path to the file that this entry represents.
    ///
    /// The full path is created by joining the original path to `read_dir`
    /// with the filename of this entry.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::fs;
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut entries = fs::read_dir(".").await?;
    ///
    /// while let Some(res) = entries.next().await {
    ///     let entry = res?;
    ///     println!("{:?}", entry.path());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// This prints output like:
    ///
    /// ```text
    /// "./whatever.txt"
    /// "./foo.html"
    /// "./hello_world.rs"
    /// ```
    ///
    /// The exact text, of course, depends on what files you have in `.`.
    pub fn path(&self) -> PathBuf {
        self.0.path()
    }

    /// Returns the bare file name of this directory entry without any other
    /// leading path component.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(async_await)]
    ///
    /// use tokio::fs;
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut entries = fs::read_dir(".").await?;
    ///
    /// while let Some(res) = entries.next().await {
    ///     let entry = res?;
    ///     println!("{:?}", entry.file_name());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn file_name(&self) -> OsString {
        self.0.file_name()
    }

    /// Return the metadata for the file that this entry points at.
    ///
    /// This function will not traverse symlinks if this entry points at a
    /// symlink.
    ///
    /// # Platform-specific behavior
    ///
    /// On Windows this function is cheap to call (no extra system calls
    /// needed), but on Unix platforms this function is the equivalent of
    /// calling `symlink_metadata` on the path.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(async_await)]
    ///
    /// use tokio::fs;
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut entries = fs::read_dir(".").await?;
    ///
    /// while let Some(res) = entries.next().await {
    ///     let entry = res?;
    ///
    ///     if let Ok(metadata) = entry.metadata().await {
    ///         // Now let's show our entry's permissions!
    ///         println!("{:?}: {:?}", entry.path(), metadata.permissions());
    ///     } else {
    ///         println!("Couldn't get file type for {:?}", entry.path());
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::needless_lifetimes)] // false positive: https://github.com/rust-lang/rust-clippy/issues/3988
    pub async fn metadata(&self) -> io::Result<Metadata> {
        asyncify(|| self.0.metadata()).await
    }

    /// Return the file type for the file that this entry points at.
    ///
    /// This function will not traverse symlinks if this entry points at a
    /// symlink.
    ///
    /// # Platform-specific behavior
    ///
    /// On Windows and most Unix platforms this function is free (no extra
    /// system calls needed), but some Unix platforms may require the equivalent
    /// call to `symlink_metadata` to learn about the target file type.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(async_await)]
    ///
    /// use tokio::fs;
    /// use tokio::prelude::*;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut entries = fs::read_dir(".").await?;
    ///
    /// while let Some(res) = entries.next().await {
    ///     let entry = res?;
    ///
    ///     if let Ok(file_type) = entry.file_type().await {
    ///         // Now let's show our entry's file type!
    ///         println!("{:?}: {:?}", entry.path(), file_type);
    ///     } else {
    ///         println!("Couldn't get file type for {:?}", entry.path());
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::needless_lifetimes)] // false positive: https://github.com/rust-lang/rust-clippy/issues/3988
    pub async fn file_type(&self) -> io::Result<FileType> {
        asyncify(|| self.0.file_type()).await
    }
}

#[cfg(unix)]
impl DirEntryExt for DirEntry {
    fn ino(&self) -> u64 {
        self.0.ino()
    }
}
