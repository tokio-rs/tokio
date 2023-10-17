use std::io::{BufRead, Read, Seek, Write};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite,
    AsyncWriteExt,
};

/// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
/// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
#[derive(Debug)]
pub struct SyncIoBridge<T> {
    src: T,
    rt: tokio::runtime::Handle,
}

impl<T: AsyncBufRead + Unpin> BufRead for SyncIoBridge<T> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        let src = &mut self.src;
        self.rt.block_on(AsyncBufReadExt::fill_buf(src))
    }

    fn consume(&mut self, amt: usize) {
        let src = &mut self.src;
        AsyncBufReadExt::consume(src, amt)
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt
            .block_on(AsyncBufReadExt::read_until(src, byte, buf))
    }
    fn read_line(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(AsyncBufReadExt::read_line(src, buf))
    }
}

impl<T: AsyncRead + Unpin> Read for SyncIoBridge<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(AsyncReadExt::read(src, buf))
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.read_to_end(buf))
    }

    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.read_to_string(buf))
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let src = &mut self.src;
        // The AsyncRead trait returns the count, synchronous doesn't.
        let _n = self.rt.block_on(src.read_exact(buf))?;
        Ok(())
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

impl<T: AsyncSeek + Unpin> Seek for SyncIoBridge<T> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let src = &mut self.src;
        self.rt.block_on(AsyncSeekExt::seek(src, pos))
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

impl<T: AsyncWrite + Unpin> SyncIoBridge<T> {
    /// Shutdown this writer. This method provides a way to call the [`AsyncWriteExt::shutdown`]
    /// function of the inner [`tokio::io::AsyncWrite`] instance.
    ///
    /// # Errors
    ///
    /// This method returns the same errors as [`AsyncWriteExt::shutdown`].
    ///
    /// [`AsyncWriteExt::shutdown`]: tokio::io::AsyncWriteExt::shutdown
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        let src = &mut self.src;
        self.rt.block_on(src.shutdown())
    }
}

impl<T: Unpin> SyncIoBridge<T> {
    /// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
    /// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
    ///
    /// When this struct is created, it captures a handle to the current thread's runtime with [`tokio::runtime::Handle::current`].
    /// It is hence OK to move this struct into a separate thread outside the runtime, as created
    /// by e.g. [`tokio::task::spawn_blocking`].
    ///
    /// Stated even more strongly: to make use of this bridge, you *must* move
    /// it into a separate thread outside the runtime.  The synchronous I/O will use the
    /// underlying handle to block on the backing asynchronous source, via
    /// [`tokio::runtime::Handle::block_on`].  As noted in the documentation for that
    /// function, an attempt to `block_on` from an asynchronous execution context
    /// will panic.
    ///
    /// # Wrapping `!Unpin` types
    ///
    /// Use e.g. `SyncIoBridge::new(Box::pin(src))`.
    ///
    /// # Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime.
    #[track_caller]
    pub fn new(src: T) -> Self {
        Self::new_with_handle(src, tokio::runtime::Handle::current())
    }

    /// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
    /// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
    ///
    /// This is the same as [`SyncIoBridge::new`], but allows passing an arbitrary handle and hence may
    /// be initially invoked outside of an asynchronous context.
    pub fn new_with_handle(src: T, rt: tokio::runtime::Handle) -> Self {
        Self { src, rt }
    }

    /// Consume this bridge, returning the underlying stream.
    pub fn into_inner(self) -> T {
        self.src
    }
}
