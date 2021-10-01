use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
/// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
#[derive(Debug)]
pub struct SyncIoBridge<T> {
    src: T,
    rt: tokio::runtime::Handle,
}

impl<T: AsyncRead + Unpin> Read for SyncIoBridge<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.read(buf))
    }
}

impl<T: AsyncWrite + Unpin> Write for SyncIoBridge<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let src = &mut self.src;
        self.rt.block_on(src.flush())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let src = &mut self.src;
        self.rt.block_on(src.write_all(buf))
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.write_vectored(bufs))
    }
}

// Because https://doc.rust-lang.org/std/io/trait.Write.html#method.is_write_vectored is at the time
// of this writing still unstable, we expose this as part of a standalone method.
impl<T: AsyncWrite> SyncIoBridge<T> {
    /// Determines if the underlying [`tokio::io::AsyncWrite`] target supports efficient vectored writes.
    ///
    /// See [`tokio::io::AsyncWrite::is_write_vectored`].
    pub fn is_write_vectored(&self) -> bool {
        self.src.is_write_vectored()
    }
}

impl<T: AsyncRead + Unpin> SyncIoBridge<T> {
    /// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
    /// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
    ///
    /// This is useful with e.g. [`tokio::task::spawn_blocking`].
    ///
    /// This works via capturing a handle to the current thread's runtime with [`tokio::runtime::Handle::current`].
    /// Synchronous I/O will use that handle to block on the backing asynchronous source.
    ///
    /// # Panic
    ///
    /// This will panic if called outside the context of a Tokio runtime.
    pub fn new(src: T) -> Self {
        Self::new_with_handle(src, tokio::runtime::Handle::current())
    }

    /// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
    /// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
    ///
    /// This is the same as [`SyncIoBridge::new`], but allows passing an arbitrary handle and hence may
    /// be invoked outside of an asynchronous context.
    pub fn new_with_handle(src: T, rt: tokio::runtime::Handle) -> Self {
        Self { src, rt }
    }
}
