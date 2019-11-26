use crate::io::util::flush::{flush, Flush};
use crate::io::util::shutdown::{shutdown, Shutdown};
use crate::io::util::write::{write, Write};
use crate::io::util::write_all::{write_all, WriteAll};
use crate::io::AsyncWrite;

cfg_io_util! {
    /// Write bytes to a sink.
    ///
    /// Implemented as an extention trait, adding utility methods to all
    /// [`AsyncWrite`] types. Callers will tend to import this trait instead of
    /// [`AsyncWrite`].
    ///
    /// As a convenience, this trait may be imported using the [`prelude`]:
    ///
    /// ```no_run
    /// use tokio::prelude::*;
    /// use tokio::fs::File;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let data = b"some bytes";
    ///
    ///     let mut pos = 0;
    ///     let mut buffer = File::create("foo.txt").await?;
    ///
    ///     while pos < data.len() {
    ///         let bytes_written = buffer.write(&data[pos..]).await?;
    ///         pos += bytes_written;
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// See [module][crate::io] documentation for more details.
    ///
    /// [`AsyncWrite`]: AsyncWrite
    /// [`prelude`]: crate::prelude
    pub trait AsyncWriteExt: AsyncWrite {
        /// Write a buffer into this writer, returning how many bytes were
        /// written.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
        /// ```
        ///
        /// This function will attempt to write the entire contents of `buf`, but
        /// the entire write may not succeed, or the write may also generate an
        /// error. A call to `write` represents *at most one* attempt to write to
        /// any wrapped object.
        ///
        /// If the return value is `Ok(n)` then it must be guaranteed that `n <=
        /// buf.len()`. A return value of `0` typically means that the
        /// underlying object is no longer able to accept bytes and will likely
        /// not be able to in the future as well, or that the buffer provided is
        /// empty.
        ///
        /// # Errors
        ///
        /// Each call to `write` may generate an I/O error indicating that the
        /// operation could not be completed. If an error is returned then no bytes
        /// in the buffer were written to this writer.
        ///
        /// It is **not** considered an error if the entire buffer could not be
        /// written to this writer.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tokio::io::{self, AsyncWriteExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut buffer = File::create("foo.txt").await?;
        ///
        ///     // Writes some prefix of the byte string, not necessarily all of it.
        ///     buffer.write(b"some bytes").await?;
        ///     Ok(())
        /// }
        /// ```
        fn write<'a>(&'a mut self, src: &'a [u8]) -> Write<'a, Self>
        where
            Self: Unpin,
        {
            write(self, src)
        }

        /// Attempts to write an entire buffer into this writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;
        /// ```
        ///
        /// This method will continuously call [`write`] until there is no more data
        /// to be written. This method will not return until the entire buffer
        /// has been successfully written or such an error occurs. The first
        /// error generated from this method will be returned.
        ///
        /// # Errors
        ///
        /// This function will return the first error that [`write`] returns.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tokio::io::{self, AsyncWriteExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut buffer = File::create("foo.txt").await?;
        ///
        ///     buffer.write_all(b"some bytes").await?;
        ///     Ok(())
        /// }
        /// ```
        ///
        /// [`write`]: AsyncWriteExt::write
        fn write_all<'a>(&'a mut self, src: &'a [u8]) -> WriteAll<'a, Self>
        where
            Self: Unpin,
        {
            write_all(self, src)
        }

        /// Flush this output stream, ensuring that all intermediately buffered
        /// contents reach their destination.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn flush(&mut self) -> io::Result<()>;
        /// ```
        ///
        /// # Errors
        ///
        /// It is considered an error if not all bytes could be written due to
        /// I/O errors or EOF being reached.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tokio::io::{self, BufWriter, AsyncWriteExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let f = File::create("foo.txt").await?;
        ///     let mut buffer = BufWriter::new(f);
        ///
        ///     buffer.write_all(b"some bytes").await?;
        ///     buffer.flush().await?;
        ///     Ok(())
        /// }
        /// ```
        fn flush(&mut self) -> Flush<'_, Self>
        where
            Self: Unpin,
        {
            flush(self)
        }

        /// Shuts down the output stream, ensuring that the value can be dropped
        /// cleanly.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn shutdown(&mut self) -> io::Result<()>;
        /// ```
        ///
        /// Similar to [`flush`], all intermediately buffered is written to the
        /// underlying stream. Once the operation completes, the caller should
        /// no longer attempt to write to the stream. For example, the
        /// `TcpStream` implementation will issue a `shutdown(Write)` sys call.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use tokio::io::{self, BufWriter, AsyncWriteExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let f = File::create("foo.txt").await?;
        ///     let mut buffer = BufWriter::new(f);
        ///
        ///     buffer.write_all(b"some bytes").await?;
        ///     buffer.shutdown().await?;
        ///     Ok(())
        /// }
        /// ```
        fn shutdown(&mut self) -> Shutdown<'_, Self>
        where
            Self: Unpin,
        {
            shutdown(self)
        }
    }
}

impl<W: AsyncWrite + ?Sized> AsyncWriteExt for W {}
