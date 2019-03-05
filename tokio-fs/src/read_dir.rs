use super::blocking_pool::{blocking, Blocking};
use futures::{future, try_ready, Async, Future, Poll, Stream};
use std::ffi::OsString;
use std::fs::{self, DirEntry as StdDirEntry, FileType, Metadata, ReadDir as StdReadDir};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::DirEntryExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
pub struct ReadDirFuture<P>(ReadDirFutureMode<P>)
where
    P: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum ReadDirFutureMode<P>
where
    P: AsRef<Path> + Send + 'static,
{
    Native { path: P },
    Fallback(Blocking<StdReadDir, io::Error>),
}

impl<P> ReadDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> ReadDirFuture<P> {
        ReadDirFuture(if tokio_threadpool::entered() {
            ReadDirFutureMode::Native { path }
        } else {
            ReadDirFutureMode::Fallback(blocking(future::lazy(move || fs::read_dir(&path))))
        })
    }
}

impl<P> Future for ReadDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = ReadDir;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, io::Error> {
        let std = match &mut self.0 {
            ReadDirFutureMode::Native { path } => {
                try_ready!(crate::blocking_io(|| fs::read_dir(path)))
            }
            ReadDirFutureMode::Fallback(job) => try_ready!(job.poll()),
        };

        let read_dir = ReadDir(if tokio_threadpool::entered() {
            ReadDirMode::Native(std)
        } else {
            ReadDirMode::Fallback(Some(ReadDirStatus::Idle(std)))
        });
        Ok(read_dir.into())
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
pub struct ReadDir(ReadDirMode);

#[derive(Debug)]
enum ReadDirMode {
    Native(StdReadDir),
    Fallback(Option<ReadDirStatus>),
}

#[derive(Debug)]
enum ReadDirStatus {
    Idle(StdReadDir),
    Blocked(Blocking<(StdReadDir, Option<StdDirEntry>), (StdReadDir, io::Error)>),
}

impl Stream for ReadDir {
    type Item = DirEntry;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match &mut self.0 {
                ReadDirMode::Native(std) => {
                    return crate::blocking_io(|| match std.next() {
                        Some(Err(err)) => Err(err),
                        Some(Ok(item)) => Ok(Some(DirEntry::from_std(item))),
                        None => Ok(None),
                    })
                }
                ReadDirMode::Fallback(fallback) => match fallback.take().unwrap() {
                    ReadDirStatus::Idle(mut std) => {
                        // Start a new blocking task that fetches the next `DirEntry`.
                        *fallback = Some(ReadDirStatus::Blocked(blocking(future::lazy(
                            move || match std.next() {
                                Some(Err(err)) => Err((std, err)),
                                Some(Ok(item)) => Ok((std, Some(item))),
                                None => Ok((std, None)),
                            },
                        ))));
                    }
                    ReadDirStatus::Blocked(mut job) => match job.poll() {
                        Ok(Async::Ready((std, item))) => {
                            *fallback = Some(ReadDirStatus::Idle(std));
                            return Ok(Async::Ready(item.map(DirEntry::from_std)));
                        }
                        Ok(Async::NotReady) => {
                            *fallback = Some(ReadDirStatus::Blocked(job));
                            return Ok(Async::NotReady);
                        }
                        Err((std, err)) => {
                            *fallback = Some(ReadDirStatus::Idle(std));
                            return Err(err);
                        }
                    },
                },
            }
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
pub struct DirEntry(DirEntryMode);

#[derive(Debug)]
enum DirEntryMode {
    Native(StdDirEntry),
    Fallback(Mutex<Option<DirEntryStatus>>),
}

#[derive(Debug)]
enum DirEntryStatus {
    Idle(StdDirEntry),
    Blocked(Blocking<(StdDirEntry, Operation), (StdDirEntry, io::Error)>),
}

#[derive(Debug)]
enum Operation {
    Metadata(Metadata),
    FileType(FileType),
}

impl DirEntry {
    fn from_std(std: StdDirEntry) -> DirEntry {
        DirEntry(if tokio_threadpool::entered() {
            DirEntryMode::Native(std)
        } else {
            DirEntryMode::Fallback(Mutex::new(Some(DirEntryStatus::Idle(std))))
        })
    }

    /// Destructures the `tokio_fs::DirEntry` into a [`std::fs::DirEntry`][std].
    ///
    /// [std]: https://doc.rust-lang.org/std/fs/struct.DirEntry.html
    pub fn into_std(self) -> StdDirEntry {
        match self.0 {
            DirEntryMode::Native(std) => std,
            DirEntryMode::Fallback(fallback) => match fallback.into_inner().unwrap().unwrap() {
                DirEntryStatus::Idle(std) => std,
                DirEntryStatus::Blocked(..) => {
                    panic!("`DirEntry` instance blocked on an async operation")
                }
            },
        }
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
        match &self.0 {
            DirEntryMode::Native(std) => std.path(),
            DirEntryMode::Fallback(fallback) => {
                let fallback = &*fallback.lock().unwrap_or_else(|err| err.into_inner());

                match fallback.as_ref().unwrap() {
                    DirEntryStatus::Idle(std) => std.path(),
                    DirEntryStatus::Blocked(..) => {
                        panic!("`DirEntry` instance blocked on an async operation")
                    }
                }
            }
        }
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
        match &self.0 {
            DirEntryMode::Native(std) => std.file_name(),
            DirEntryMode::Fallback(fallback) => {
                let fallback = &*fallback.lock().unwrap_or_else(|err| err.into_inner());

                match fallback.as_ref().unwrap() {
                    DirEntryStatus::Idle(std) => std.file_name(),
                    DirEntryStatus::Blocked(..) => {
                        panic!("`DirEntry` instance blocked on an async operation")
                    }
                }
            }
        }
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
    pub fn poll_metadata(&self) -> Poll<Metadata, io::Error> {
        match &self.0 {
            DirEntryMode::Native(std) => crate::blocking_io(|| std.metadata()),
            DirEntryMode::Fallback(fallback) => {
                let fallback = &mut *fallback.lock().unwrap_or_else(|err| err.into_inner());
                loop {
                    match fallback.take().unwrap() {
                        DirEntryStatus::Idle(std) => {
                            // Start a new blocking task that fetches the metadata.
                            *fallback = Some(DirEntryStatus::Blocked(blocking(future::lazy(
                                move || match std.metadata() {
                                    Ok(md) => Ok((std, Operation::Metadata(md))),
                                    Err(err) => Err((std, err)),
                                },
                            ))));
                        }
                        DirEntryStatus::Blocked(mut job) => match job.poll() {
                            Ok(Async::Ready((std, oper))) => {
                                *fallback = Some(DirEntryStatus::Idle(std));

                                // If the last operation was `MetaData`, returns its result.
                                if let Operation::Metadata(md) = oper {
                                    return Ok(md.into());
                                }
                            }
                            Ok(Async::NotReady) => {
                                *fallback = Some(DirEntryStatus::Blocked(job));
                                return Ok(Async::NotReady);
                            }
                            Err(..) => unreachable!(),
                        },
                    }
                }
            }
        }
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
    pub fn poll_file_type(&self) -> Poll<FileType, io::Error> {
        match &self.0 {
            DirEntryMode::Native(std) => crate::blocking_io(|| std.file_type()),
            DirEntryMode::Fallback(fallback) => {
                let fallback = &mut *fallback.lock().unwrap_or_else(|err| err.into_inner());

                loop {
                    match fallback.take().unwrap() {
                        DirEntryStatus::Idle(std) => {
                            // Start a new blocking task that fetches the file type.
                            *fallback = Some(DirEntryStatus::Blocked(blocking(future::lazy(
                                move || match std.file_type() {
                                    Ok(ft) => Ok((std, Operation::FileType(ft))),
                                    Err(err) => Err((std, err)),
                                },
                            ))));
                        }
                        DirEntryStatus::Blocked(mut job) => match job.poll() {
                            Ok(Async::Ready((std, oper))) => {
                                *fallback = Some(DirEntryStatus::Idle(std));

                                // If the last operation was `FileType`, returns its result.
                                if let Operation::FileType(ft) = oper {
                                    return Ok(ft.into());
                                }
                            }
                            Ok(Async::NotReady) => {
                                *fallback = Some(DirEntryStatus::Blocked(job));
                                return Ok(Async::NotReady);
                            }
                            Err(..) => unreachable!(),
                        },
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
impl DirEntryExt for DirEntry {
    fn ino(&self) -> u64 {
        match &self.0 {
            DirEntryMode::Native(std) => std.ino(),
            DirEntryMode::Fallback(fallback) => {
                let fallback = &*fallback.lock().unwrap_or_else(|err| err.into_inner());

                match fallback.as_ref().unwrap() {
                    DirEntryStatus::Idle(std) => std.ino(),
                    DirEntryStatus::Blocked(..) => {
                        panic!("`DirEntry` instance blocked on an async operation")
                    }
                }
            }
        }
    }
}
