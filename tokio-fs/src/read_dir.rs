use futures_core::stream::Stream;
use std::ffi::OsString;
use std::fs::{self, DirEntry as StdDirEntry, FileType, Metadata, ReadDir as StdReadDir};
use std::future::Future;
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
pub fn read_dir<P>(path: P) -> ReadDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    ReadDirFuture::new(path)
}

/// Future returned by `read_dir`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReadDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    path: P,
}

impl<P> ReadDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> ReadDirFuture<P> {
        ReadDirFuture { path }
    }
}

impl<P> Future for ReadDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Output = io::Result<ReadDir>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::blocking_io(|| Ok(ReadDir(fs::read_dir(&self.path)?)))
    }
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
pub struct ReadDir(StdReadDir);

impl Stream for ReadDir {
    type Item = io::Result<DirEntry>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = Pin::get_mut(self);
        match crate::blocking_io(|| match inner.0.next() {
            Some(Err(err)) => Err(err),
            Some(Ok(item)) => Ok(Some(Ok(DirEntry(item)))),
            None => Ok(None),
        }) {
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
    /// ```
    /// use futures::{Future, Stream};
    ///
    /// let fut = tokio_fs::read_dir(".").flatten_stream().for_each(|dir| {
    ///     println!("{:?}", dir.path());
    ///     Ok(())
    /// }).map_err(|err| { eprintln!("Error: {:?}", err); () });
    ///
    /// tokio::run(fut);
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
    /// use futures::{Future, Stream};
    ///
    /// let fut = tokio_fs::read_dir(".").flatten_stream().for_each(|dir| {
    ///     // Here, `dir` is a `DirEntry`.
    ///     println!("{:?}", dir.file_name());
    ///     Ok(())
    /// }).map_err(|err| { eprintln!("Error: {:?}", err); () });
    ///
    /// tokio::run(fut);
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
    /// use futures::{Future, Stream};
    /// use futures::future::poll_fn;
    ///
    /// let fut = tokio_fs::read_dir(".").flatten_stream().for_each(|dir| {
    ///     // Here, `dir` is a `DirEntry`.
    ///     let path = dir.path();
    ///     poll_fn(move || dir.poll_metadata()).map(move |metadata| {
    ///         println!("{:?}: {:?}", path, metadata.permissions());
    ///     })
    /// }).map_err(|err| { eprintln!("Error: {:?}", err); () });
    ///
    /// tokio::run(fut);
    /// ```
    pub fn poll_metadata(&self) -> Poll<io::Result<Metadata>> {
        crate::blocking_io(|| self.0.metadata())
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
    /// use futures::{Future, Stream};
    /// use futures::future::poll_fn;
    ///
    /// let fut = tokio_fs::read_dir(".").flatten_stream().for_each(|dir| {
    ///     // Here, `dir` is a `DirEntry`.
    ///     let path = dir.path();
    ///     poll_fn(move || dir.poll_file_type()).map(move |file_type| {
    ///         // Now let's show our entry's file type!
    ///         println!("{:?}: {:?}", path, file_type);
    ///     })
    /// }).map_err(|err| { eprintln!("Error: {:?}", err); () });
    ///
    /// tokio::run(fut);
    /// ```
    pub fn poll_file_type(&self) -> Poll<io::Result<FileType>> {
        crate::blocking_io(|| self.0.file_type())
    }
}

#[cfg(unix)]
impl DirEntryExt for DirEntry {
    fn ino(&self) -> u64 {
        self.0.ino()
    }
}
