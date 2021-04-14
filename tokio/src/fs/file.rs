//! Types for working with [`File`].
//!
//! [`File`]: File

use self::State::*;
use crate::fs::{asyncify, sys};
use crate::io::blocking::Buf;
use crate::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
use crate::sync::Mutex;

use std::fmt;
use std::fs::{Metadata, Permissions};
use std::future::Future;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::task::Poll::*;

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
    std: Arc<sys::File>,
    inner: Mutex<Inner>,
}

struct Inner {
    state: State,

    /// Errors from writes/flushes are returned in write/flush calls. If a write
    /// error is observed while performing a read, it is saved until the next
    /// write / flush call.
    last_write_err: Option<io::ErrorKind>,

    pos: u64,
}

#[derive(Debug)]
enum State {
    Idle(Option<Buf>),
    Busy(sys::Blocking<(Operation, Buf)>),
}

#[derive(Debug)]
enum Operation {
    Read(io::Result<usize>),
    Write(io::Result<()>),
    Seek(io::Result<u64>),
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
            std: Arc::new(std),
            inner: Mutex::new(Inner {
                state: State::Idle(Some(Buf::with_capacity(0))),
                last_write_err: None,
                pos: 0,
            }),
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
        let mut inner = self.inner.lock().await;
        inner.complete_inflight().await;

        let std = self.std.clone();
        asyncify(move || std.sync_all()).await
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
        let mut inner = self.inner.lock().await;
        inner.complete_inflight().await;

        let std = self.std.clone();
        asyncify(move || std.sync_data()).await
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
        let mut inner = self.inner.lock().await;
        inner.complete_inflight().await;

        let mut buf = match inner.state {
            Idle(ref mut buf_cell) => buf_cell.take().unwrap(),
            _ => unreachable!(),
        };

        let seek = if !buf.is_empty() {
            Some(SeekFrom::Current(buf.discard_read()))
        } else {
            None
        };

        let std = self.std.clone();

        inner.state = Busy(sys::run(move || {
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
            Idle(_) => unreachable!(),
            Busy(ref mut rx) => rx.await?,
        };

        inner.state = Idle(Some(buf));

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
        let std = self.std.clone();
        asyncify(move || std.metadata()).await
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
        let std = self.std.clone();
        let std_file = asyncify(move || std.try_clone()).await?;
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
    pub async fn into_std(mut self) -> sys::File {
        self.inner.get_mut().complete_inflight().await;
        Arc::try_unwrap(self.std).expect("Arc::try_unwrap failed")
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
    pub fn try_into_std(mut self) -> Result<sys::File, Self> {
        match Arc::try_unwrap(self.std) {
            Ok(file) => Ok(file),
            Err(std_file_arc) => {
                self.std = std_file_arc;
                Err(self)
            }
        }
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
        let std = self.std.clone();
        asyncify(move || std.set_permissions(perm)).await
    }
}

impl AsyncRead for File {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        dst: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        let inner = me.inner.get_mut();

        loop {
            match inner.state {
                Idle(ref mut buf_cell) => {
                    let buf = buf_cell.as_mut().unwrap();

                    if !buf.is_empty() {
                        buf.copy_to(dst);
                        return Ready(Ok(()));
                    }

                    if let Some(x) = try_nonblocking_read(me.std.as_ref(), dst) {
                        return Ready(x);
                    }

                    let mut buf = buf_cell.take().unwrap();
                    buf.ensure_capacity_for(dst);
                    let std = me.std.clone();

                    inner.state = Busy(sys::run(move || {
                        let res = buf.read_from(&mut &*std);
                        (Operation::Read(res), buf)
                    }));
                }
                Busy(ref mut rx) => {
                    let (op, mut buf) = ready!(Pin::new(rx).poll(cx))?;

                    match op {
                        Operation::Read(Ok(_)) => {
                            buf.copy_to(dst);
                            inner.state = Idle(Some(buf));
                            return Ready(Ok(()));
                        }
                        Operation::Read(Err(e)) => {
                            assert!(buf.is_empty());

                            inner.state = Idle(Some(buf));
                            return Ready(Err(e));
                        }
                        Operation::Write(Ok(_)) => {
                            assert!(buf.is_empty());
                            inner.state = Idle(Some(buf));
                            continue;
                        }
                        Operation::Write(Err(e)) => {
                            assert!(inner.last_write_err.is_none());
                            inner.last_write_err = Some(e.kind());
                            inner.state = Idle(Some(buf));
                        }
                        Operation::Seek(result) => {
                            assert!(buf.is_empty());
                            inner.state = Idle(Some(buf));
                            if let Ok(pos) = result {
                                inner.pos = pos;
                            }
                            continue;
                        }
                    }
                }
            }
        }
    }
}

impl AsyncSeek for File {
    fn start_seek(self: Pin<&mut Self>, mut pos: SeekFrom) -> io::Result<()> {
        let me = self.get_mut();
        let inner = me.inner.get_mut();

        loop {
            match inner.state {
                Busy(_) => panic!("must wait for poll_complete before calling start_seek"),
                Idle(ref mut buf_cell) => {
                    let mut buf = buf_cell.take().unwrap();

                    // Factor in any unread data from the buf
                    if !buf.is_empty() {
                        let n = buf.discard_read();

                        if let SeekFrom::Current(ref mut offset) = pos {
                            *offset += n;
                        }
                    }

                    let std = me.std.clone();

                    inner.state = Busy(sys::run(move || {
                        let res = (&*std).seek(pos);
                        (Operation::Seek(res), buf)
                    }));
                    return Ok(());
                }
            }
        }
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let inner = self.inner.get_mut();

        loop {
            match inner.state {
                Idle(_) => return Poll::Ready(Ok(inner.pos)),
                Busy(ref mut rx) => {
                    let (op, buf) = ready!(Pin::new(rx).poll(cx))?;
                    inner.state = Idle(Some(buf));

                    match op {
                        Operation::Read(_) => {}
                        Operation::Write(Err(e)) => {
                            assert!(inner.last_write_err.is_none());
                            inner.last_write_err = Some(e.kind());
                        }
                        Operation::Write(_) => {}
                        Operation::Seek(res) => {
                            if let Ok(pos) = res {
                                inner.pos = pos;
                            }
                            return Ready(res);
                        }
                    }
                }
            }
        }
    }
}

impl AsyncWrite for File {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        src: &[u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        let inner = me.inner.get_mut();

        if let Some(e) = inner.last_write_err.take() {
            return Ready(Err(e.into()));
        }

        loop {
            match inner.state {
                Idle(ref mut buf_cell) => {
                    let mut buf = buf_cell.take().unwrap();

                    let seek = if !buf.is_empty() {
                        Some(SeekFrom::Current(buf.discard_read()))
                    } else {
                        None
                    };

                    let n = buf.copy_from(src);
                    let std = me.std.clone();

                    inner.state = Busy(sys::run(move || {
                        let res = if let Some(seek) = seek {
                            (&*std).seek(seek).and_then(|_| buf.write_to(&mut &*std))
                        } else {
                            buf.write_to(&mut &*std)
                        };

                        (Operation::Write(res), buf)
                    }));

                    return Ready(Ok(n));
                }
                Busy(ref mut rx) => {
                    let (op, buf) = ready!(Pin::new(rx).poll(cx))?;
                    inner.state = Idle(Some(buf));

                    match op {
                        Operation::Read(_) => {
                            // We don't care about the result here. The fact
                            // that the cursor has advanced will be reflected in
                            // the next iteration of the loop
                            continue;
                        }
                        Operation::Write(res) => {
                            // If the previous write was successful, continue.
                            // Otherwise, error.
                            res?;
                            continue;
                        }
                        Operation::Seek(_) => {
                            // Ignore the seek
                            continue;
                        }
                    }
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let inner = self.inner.get_mut();
        inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.poll_flush(cx)
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
            .field("std", &self.std)
            .finish()
    }
}

#[cfg(unix)]
impl std::os::unix::io::AsRawFd for File {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.std.as_raw_fd()
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
        self.std.as_raw_handle()
    }
}

#[cfg(windows)]
impl std::os::windows::io::FromRawHandle for File {
    unsafe fn from_raw_handle(handle: std::os::windows::io::RawHandle) -> Self {
        sys::File::from_raw_handle(handle).into()
    }
}

impl Inner {
    async fn complete_inflight(&mut self) {
        use crate::future::poll_fn;

        if let Err(e) = poll_fn(|cx| Pin::new(&mut *self).poll_flush(cx)).await {
            self.last_write_err = Some(e.kind());
        }
    }

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if let Some(e) = self.last_write_err.take() {
            return Ready(Err(e.into()));
        }

        let (op, buf) = match self.state {
            Idle(_) => return Ready(Ok(())),
            Busy(ref mut rx) => ready!(Pin::new(rx).poll(cx))?,
        };

        // The buffer is not used here
        self.state = Idle(Some(buf));

        match op {
            Operation::Read(_) => Ready(Ok(())),
            Operation::Write(res) => Ready(res),
            Operation::Seek(_) => Ready(Ok(())),
        }
    }
}

#[cfg(all(target_os = "linux", not(test)))]
pub(crate) fn try_nonblocking_read(
    file: &crate::fs::sys::File,
    dst: &mut ReadBuf<'_>,
) -> Option<std::io::Result<()>> {
    use std::sync::atomic::{AtomicBool, Ordering};

    static NONBLOCKING_READ_SUPPORTED: AtomicBool = AtomicBool::new(true);
    if !NONBLOCKING_READ_SUPPORTED.load(Ordering::Relaxed) {
        return None;
    }
    let out = preadv2::preadv2_safe(file, dst, -1, preadv2::RWF_NOWAIT);
    if let Err(err) = &out {
        match err.raw_os_error() {
            Some(libc::ENOSYS) => {
                NONBLOCKING_READ_SUPPORTED.store(false, Ordering::Relaxed);
                return None;
            }
            Some(libc::ENOTSUP) | Some(libc::EAGAIN) => return None,
            _ => {}
        }
    }
    Some(out)
}

#[cfg(any(not(target_os = "linux"), test))]
pub(crate) fn try_nonblocking_read(
    _file: &crate::fs::sys::File,
    _dst: &mut ReadBuf<'_>,
) -> Option<std::io::Result<()>> {
    None
}

#[cfg(target_os = "linux")]
mod preadv2 {
    use libc::{c_int, c_long, c_void, iovec, off_t, ssize_t};
    use std::os::unix::prelude::AsRawFd;

    use crate::io::ReadBuf;

    pub(crate) fn preadv2_safe(
        file: &std::fs::File,
        dst: &mut ReadBuf<'_>,
        offset: off_t,
        flags: c_int,
    ) -> std::io::Result<()> {
        unsafe {
            /* We have to defend against buffer overflows manually here.  The slice API makes
             * this fairly straightforward. */
            let unfilled = dst.unfilled_mut();
            let mut iov = iovec {
                iov_base: unfilled.as_mut_ptr() as *mut c_void,
                iov_len: unfilled.len(),
            };
            /* We take a File object rather than an fd as reading from a sensitive fd may confuse
             * other unsafe code that assumes that only they have access to that fd. */
            let bytes_read = preadv2(
                file.as_raw_fd(),
                &mut iov as *mut iovec as *const iovec,
                1,
                offset,
                flags,
            );
            if bytes_read < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                /* preadv2 returns the number of bytes read, e.g. the number of bytes that have
                 * written into `unfilled`. So it's safe to assume that the data is now
                 * initialised */
                dst.assume_init(bytes_read as usize);
                dst.advance(bytes_read as usize);
                Ok(())
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_preadv2_safe() {
            use std::io::{Seek, Write};
            use std::mem::MaybeUninit;
            use tempfile::tempdir;

            let tmp = tempdir().unwrap();
            let filename = tmp.path().join("file");
            const MESSAGE: &[u8] = b"Hello this is a test";
            {
                let mut f = std::fs::File::create(&filename).unwrap();
                f.write_all(MESSAGE).unwrap();
            }
            let f = std::fs::File::open(&filename).unwrap();

            let mut buf = [MaybeUninit::<u8>::new(0); 50];
            let mut br = ReadBuf::uninit(&mut buf);

            // Basic use:
            preadv2_safe(&f, &mut br, 0, 0).unwrap();
            assert_eq!(br.initialized().len(), MESSAGE.len());
            assert_eq!(br.filled(), MESSAGE);

            // Here we check that offset works, but also that appending to a non-empty buffer
            // behaves correctly WRT initialisation.
            preadv2_safe(&f, &mut br, 5, 0).unwrap();
            assert_eq!(br.initialized().len(), MESSAGE.len() * 2 - 5);
            assert_eq!(br.filled(), b"Hello this is a test this is a test".as_ref());

            // offset of -1 means use the current cursor.  This has not been advanced by the
            // previous reads because we specified an offset there.
            preadv2_safe(&f, &mut br, -1, 0).unwrap();
            assert_eq!(br.remaining(), 0);
            assert_eq!(
                br.filled(),
                b"Hello this is a test this is a testHello this is a".as_ref()
            );

            // but the offset should have been advanced by that read
            br.clear();
            preadv2_safe(&f, &mut br, -1, 0).unwrap();
            assert_eq!(br.filled(), b" test");

            // This should be in cache, so RWF_NOWAIT should work, but it not being in cache
            // (EAGAIN) or not supported by the underlying filesystem (ENOTSUP) is fine too.
            br.clear();
            match preadv2_safe(&f, &mut br, 0, RWF_NOWAIT) {
                Ok(()) => assert_eq!(br.filled(), MESSAGE),
                Err(e) => assert!(matches!(
                    e.raw_os_error(),
                    Some(libc::ENOTSUP) | Some(libc::EAGAIN)
                )),
            }

            // Test handling large offsets
            {
                // I hope the underlying filesystem supports sparse files
                let mut w = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&filename)
                    .unwrap();
                w.set_len(0x1_0000_0000).unwrap();
                w.seek(std::io::SeekFrom::Start(0x1_0000_0000)).unwrap();
                w.write_all(b"This is a Large File").unwrap();
            }

            br.clear();
            preadv2_safe(&f, &mut br, 0x1_0000_0008, 0).unwrap();
            assert_eq!(br.filled(), b"a Large File");
        }
    }

    fn pos_to_lohi(offset: off_t) -> (c_long, c_long) {
        // 64-bit offset is split over high and low 32-bits on 32-bit architectures.
        // 64-bit architectures still have high and low arguments, but only the low
        // one is inspected.  See pos_from_hilo in linux/fs/read_write.c.
        const HALF_LONG_BITS: usize = core::mem::size_of::<c_long>() * 8 / 2;
        (
            offset as c_long,
            // We want to shift this off_t value by size_of::<c_long>(). We can't do
            // it in one shift because if they're both 64-bits we'd be doing u64 >> 64
            // which is implementation defined.  Instead do it in two halves:
            ((offset >> HALF_LONG_BITS) >> HALF_LONG_BITS) as c_long,
        )
    }

    pub(crate) const RWF_NOWAIT: c_int = 0x00000008;
    unsafe fn preadv2(
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
        flags: c_int,
    ) -> ssize_t {
        // Call via libc::syscall rather than libc::preadv2.  preadv2 is only supported by glibc
        // and only since v2.26.  By using syscall we don't need to worry about compatiblity with
        // old glibc versions and it will work on Android and musl too.  The downside is that you
        // can't use `LD_PRELOAD` tricks any more to intercept these calls.
        let (lo, hi) = pos_to_lohi(offset);
        libc::syscall(libc::SYS_preadv2, fd, iov, iovcnt, lo, hi, flags) as ssize_t
    }
}
