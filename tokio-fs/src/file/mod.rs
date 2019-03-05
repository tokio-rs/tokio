//! Types for working with [`File`].
//!
//! [`File`]: file/struct.File.html

mod clone;
mod create;
mod metadata;
mod open;
mod open_options;
mod seek;

use blocking_pool::{spawn_blocking, BlockingFuture};

pub use self::clone::CloneFuture;
pub use self::create::CreateFuture;
pub use self::metadata::MetadataFuture;
pub use self::open::OpenFuture;
pub use self::open_options::OpenOptions;
pub use self::seek::SeekFuture;

use tokio_io::{AsyncRead, AsyncWrite};

use futures::{Async, Future, Poll};

use std::fs::{File as StdFile, Metadata, Permissions};
use std::io::ErrorKind::WouldBlock;
use std::io::{self, Read, Seek, Write};
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
///
/// # Examples
///
/// Create a new file and asynchronously write bytes to it:
///
/// ```no_run
/// extern crate tokio;
///
/// use tokio::prelude::{AsyncWrite, Future};
///
/// fn main() {
///     let task = tokio::fs::File::create("foo.txt")
///         .and_then(|mut file| file.poll_write(b"hello, world!"))
///         .map(|res| {
///             println!("{:?}", res);
///         }).map_err(|err| eprintln!("IO error: {:?}", err));
///
///     tokio::run(task);
/// }
/// ```
///
/// Read the contents of a file into a buffer
///
/// ```no_run
/// extern crate tokio;
///
/// use tokio::prelude::{AsyncRead, Future};
///
/// fn main() {
///     let task = tokio::fs::File::open("foo.txt")
///         .and_then(|mut file| {
///             let mut contents = vec![];
///             file.read_buf(&mut contents)
///                 .map(|res| {
///                     println!("{:?}", res);
///                 })
///         }).map_err(|err| eprintln!("IO error: {:?}", err));
///     tokio::run(task);
/// }
/// ```
#[derive(Debug)]
pub struct File {
    state: Option<State>,
}

#[derive(Debug)]
struct Inner {
    std: Option<StdFile>,
    read_buf: ReadBuffer,
}

#[derive(Debug)]
enum State {
    Idle(Inner),
    Blocked(BlockingFuture<io::Result<Inner>>),
}

#[derive(Debug)]
struct ReadBuffer {
    buf: Vec<u8>,
    start: usize,
    end: usize,
}

impl ReadBuffer {
    /// Creates a new read buffer.
    fn new() -> ReadBuffer {
        ReadBuffer {
            buf: vec![],
            start: 0,
            end: 0,
        }
    }

    /// Returns the number of bytes in the buffer.
    fn len(&self) -> usize {
        self.end - self.start
    }

    /// Reads some bytes into `buf` and returns the number of bytes read.
    fn read(&mut self, buf: &mut [u8]) -> usize {
        let len = self.len().min(buf.len());
        buf[0..len].copy_from_slice(&self.buf[self.start..self.start + len]);
        self.start += len;
        len
    }

    /// Writes some bytes from `reader` into this buffer and returns the number of bytes written.
    fn write<R: Read>(&mut self, mut reader: R) -> io::Result<usize> {
        assert_eq!(self.len(), 0);

        if self.buf.is_empty() {
            self.buf = vec![0; 1024];
        }

        let res = reader.read(&mut self.buf[..]);
        if let Ok(n) = res {
            self.start = 0;
            self.end = n;
        }
        res
    }
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
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    /// fn main() {
    ///     let task = tokio::fs::File::open("foo.txt").and_then(|file| {
    ///         // do something with the file ...
    ///         file.metadata().map(|md| println!("{:?}", md))
    ///     }).map_err(|e| {
    ///         // handle errors
    ///         eprintln!("IO error: {:?}", e);
    ///     });
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn open<P>(path: P) -> OpenFuture<P>
    where
        P: AsRef<Path> + Send + 'static,
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
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|file| {
    ///             // do something with the created file ...
    ///             file.metadata().map(|md| println!("{:?}", md))
    ///         }).map_err(|e| {
    ///             // handle errors
    ///             eprintln!("IO error: {:?}", e);
    ///     });
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn create<P>(path: P) -> CreateFuture<P>
    where
        P: AsRef<Path> + Send + 'static,
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
    /// # extern crate tokio;
    /// use std::fs::File;
    ///
    /// fn main() {
    ///     let std_file = File::open("foo.txt").unwrap();
    ///     let file = tokio::fs::File::from_std(std_file);
    /// }
    /// ```
    pub fn from_std(std: StdFile) -> File {
        File {
            state: Some(State::Idle(Inner {
                std: Some(std),
                read_buf: ReadBuffer::new(),
            })),
        }
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
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    /// use std::io::SeekFrom;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::open("foo.txt")
    ///         // move cursor 6 bytes from the start of the file
    ///         .and_then(|mut file| file.poll_seek(SeekFrom::Start(6)))
    ///         .map(|res| {
    ///             println!("{:?}", res);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn poll_seek(&mut self, pos: io::SeekFrom) -> Poll<u64, io::Error> {
        ::blocking_io(|| self.std().seek(pos))
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
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    /// use std::io::SeekFrom;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|file| file.seek(SeekFrom::Start(6)))
    ///         .map(|file| {
    ///             // handle returned file ..
    ///             # println!("{:?}", file);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
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
    /// # extern crate tokio;
    /// use tokio::prelude::{AsyncWrite, Future};
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|mut file| {
    ///             file.poll_write(b"hello, world!")?;
    ///             file.poll_sync_all()
    ///         })
    ///         .map(|res| {
    ///             // handle returned result ..
    ///             # println!("{:?}", res);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::{AsyncWrite, Future};
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|mut file| {
    ///             file.poll_write(b"hello, world!")?;
    ///             file.poll_sync_data()
    ///         })
    ///         .map(|res| {
    ///             // handle returned result ..
    ///             # println!("{:?}", res);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|mut file| {
    ///             file.poll_set_len(10)
    ///         })
    ///         .map(|res| {
    ///             // handle returned result ..
    ///             # println!("{:?}", res);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn poll_set_len(&mut self, size: u64) -> Poll<(), io::Error> {
        ::blocking_io(|| self.std().set_len(size))
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|file| file.metadata())
    ///         .map(|metadata| {
    ///             println!("{:?}", metadata);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn metadata(self) -> MetadataFuture {
        MetadataFuture::new(self)
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|mut file| file.poll_metadata())
    ///         .map(|metadata| {
    ///             // metadata is of type Async::Ready<Metadata>
    ///             println!("{:?}", metadata);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn poll_metadata(&mut self) -> Poll<Metadata, io::Error> {
        ::blocking_io(|| self.std().metadata())
    }

    /// Create a new `File` instance that shares the same underlying file handle
    /// as the existing `File` instance. Reads, writes, and seeks will affect both
    /// File instances simultaneously.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|mut file| file.poll_try_clone())
    ///         .map(|clone| {
    ///             // do something with the clone
    ///             # println!("{:?}", clone);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn poll_try_clone(&mut self) -> Poll<File, io::Error> {
        ::blocking_io(|| {
            let std = self.std().try_clone()?;
            Ok(File::from_std(std))
        })
    }

    /// Create a new `File` instance that shares the same underlying file handle
    /// as the existing `File` instance. Reads, writes, and seeks will affect both
    /// File instances simultaneously.
    ///
    /// # Examples
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|file| {
    ///             file.try_clone()
    ///                 .map(|(file, clone)| {
    ///                     // do something with the file and the clone
    ///                     # println!("{:?} {:?}", file, clone);
    ///                 })
    ///                 .map_err(|(file, err)| {
    ///                     // you get the original file back if there's an error
    ///                     # println!("{:?}", file);
    ///                     err
    ///                 })
    ///         })
    ///         .map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
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
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .and_then(|file| file.metadata())
    ///         .map(|(mut file, metadata)| {
    ///             let mut perms = metadata.permissions();
    ///             perms.set_readonly(true);
    ///             match file.poll_set_permissions(perms) {
    ///                 Err(e) => eprintln!("{}", e),
    ///                 _ => println!("permissions set!"),
    ///             }
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// use tokio::prelude::Future;
    ///
    /// fn main() {
    ///     let task = tokio::fs::File::create("foo.txt")
    ///         .map(|file| {
    ///             let std_file = file.into_std();
    ///             // do something with the std::fs::File
    ///             # println!("{:?}", std_file);
    ///         }).map_err(|err| eprintln!("IO error: {:?}", err));
    ///
    ///     tokio::run(task);
    /// }
    /// ```
    pub fn into_std(mut self) -> StdFile {
        match self.state.take().unwrap() {
            State::Idle(inner) => inner.std.expect("`File` instance already shutdown"),
            State::Blocked(_) => panic!("`File` instance blocked on an async operation"),
        }
    }

    fn std(&mut self) -> &mut StdFile {
        match self.state.as_mut().unwrap() {
            State::Idle(inner) => inner
                .std
                .as_mut()
                .expect("`File` instance already shutdown"),
            State::Blocked(_) => panic!("`File` instance blocked on an async operation"),
        }
    }

    /// Polls the asynchronous task this file handle is blocked on.
    ///
    /// If it's not blocked on any asynchronous tasks, `Ok(Async::Ready(()))` is returned.
    fn poll_job(&mut self) -> Poll<(), io::Error> {
        match self.state.take().unwrap() {
            state @ State::Idle(_) => {
                self.state = Some(state);
                return Ok(Async::Ready(()));
            }
            State::Blocked(mut job) => match job.poll().unwrap() {
                Async::Ready(Ok(inner)) => {
                    self.state = Some(State::Idle(inner));
                    Ok(Async::Ready(()))
                }
                Async::Ready(Err(err)) => {
                    // TODO(stjepang): Should we restore inner here?
                    return Err(err);
                }
                Async::NotReady => {
                    self.state = Some(State::Blocked(job));
                    return Ok(Async::NotReady);
                }
            },
        }
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.poll_job() {
                Ok(Async::Ready(())) => {}
                Ok(Async::NotReady) => return Err(WouldBlock.into()),
                Err(err) => return Err(err),
            }

            match self.state.take().unwrap() {
                State::Idle(mut inner) => {
                    if inner.read_buf.len() > 0 {
                        let n = inner.read_buf.read(buf);
                        return Ok(n);
                    }

                    if tokio_threadpool::entered() {
                        self.state = Some(State::Idle(inner));
                        return ::would_block(|| self.std().read(buf));
                    }

                    let mut read_buf = inner.read_buf;
                    let mut std = inner.std;

                    self.state = Some(State::Blocked(spawn_blocking(move || {
                        read_buf
                            .write(std.as_mut().expect("`File` instance already shutdown"))
                            .map(move |_| Inner { std, read_buf })
                    })))
                }
                State::Blocked(_) => unreachable!(),
            }
        }
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
        loop {
            try_ready!(self.poll_job());

            if tokio_threadpool::entered() {
                return ::blocking_io(|| {
                    self.state = None;
                    Ok(())
                });
            }

            match self.state.take().unwrap() {
                State::Idle(mut inner) => {
                    if inner.std.is_none() {
                        return Ok(Async::Ready(()));
                    }

                    self.state = Some(State::Blocked(spawn_blocking(move || {
                        inner.std.take();
                        Ok(inner)
                    })));
                }
                State::Blocked(_) => unreachable!(),
            }
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        // This is probably fine as closing a file *shouldn't* be a blocking
        // operation. That said, ideally `shutdown` is called first.
    }
}
