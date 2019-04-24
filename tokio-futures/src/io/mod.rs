//! Use I/O with `async` / `await`.

mod flush;
mod read;
mod read_exact;
mod write;
mod write_all;

pub use self::flush::Flush;
pub use self::read::Read;
pub use self::read_exact::ReadExact;
pub use self::write::Write;
pub use self::write_all::WriteAll;

use tokio_io::{AsyncRead, AsyncWrite};

/// An extension trait which adds utility methods to `AsyncRead` types.
pub trait AsyncReadExt: AsyncRead {
    /// Tries to read some bytes directly into the given `buf` in an
    /// asynchronous manner, returning a future.
    ///
    /// The returned future will resolve to the number of bytes read once the read
    /// operation is completed.
    ///
    /// # Examples
    ///
    /// ```edition2018
    /// #![feature(async_await, await_macro, futures_api)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::AsyncReadExt;
    /// use std::io::Cursor;
    ///
    /// let mut reader = Cursor::new([1, 2, 3, 4]);
    /// let mut output = [0u8; 5];
    ///
    /// let bytes = await!(reader.read_async(&mut output[..])).unwrap();
    ///
    /// // This is only guaranteed to be 4 because `&[u8]` is a synchronous
    /// // reader. In a real system you could get anywhere from 1 to
    /// // `output.len()` bytes in a single read.
    /// assert_eq!(bytes, 4);
    /// assert_eq!(output, [1, 2, 3, 4, 0]);
    /// });
    /// ```
    fn read_async<'a>(&'a mut self, buf: &'a mut [u8]) -> Read<'a, Self> {
        Read::new(self, buf)
    }

    /// Creates a future which will read exactly enough bytes to fill `buf`,
    /// returning an error if end of file (EOF) is hit sooner.
    ///
    /// The returned future will resolve once the read operation is completed.
    ///
    /// In the case of an error the buffer and the object will be discarded, with
    /// the error yielded.
    ///
    /// # Examples
    ///
    /// ```edition2018
    /// #![feature(async_await, await_macro, futures_api)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::AsyncReadExt;
    /// use std::io::Cursor;
    ///
    /// let mut reader = Cursor::new([1, 2, 3, 4]);
    /// let mut output = [0u8; 4];
    ///
    /// await!(reader.read_exact_async(&mut output)).unwrap();
    ///
    /// assert_eq!(output, [1, 2, 3, 4]);
    /// });
    /// ```
    ///
    /// ## EOF is hit before `buf` is filled
    ///
    /// ```edition2018
    /// #![feature(async_await, await_macro, futures_api)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::AsyncReadExt;
    /// use std::io::{self, Cursor};
    ///
    /// let mut reader = Cursor::new([1, 2, 3, 4]);
    /// let mut output = [0u8; 5];
    ///
    /// let result = await!(reader.read_exact_async(&mut output));
    ///
    /// assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    /// });
    /// ```
    fn read_exact_async<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a, Self> {
        ReadExact::new(self, buf)
    }
}

/// An extension trait which adds utility methods to `AsyncWrite` types.
pub trait AsyncWriteExt: AsyncWrite {
    /// Write data into this object.
    ///
    /// Creates a future that will write the entire contents of the buffer `buf` into
    /// this `AsyncWrite`.
    ///
    /// The returned future will not complete until all the data has been written.
    ///
    /// # Examples
    ///
    /// ```edition2018
    /// #![feature(async_await, await_macro, futures_api)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::AsyncWriteExt;
    /// use std::io::Cursor;
    ///
    /// let mut buf = [0u8; 5];
    /// let mut writer = Cursor::new(&mut buf[..]);
    ///
    /// let n = await!(writer.write_async(&[1, 2, 3, 4])).unwrap();
    ///
    /// assert_eq!(writer.into_inner()[..n], [1, 2, 3, 4, 0][..n]);
    /// });
    /// ```
    fn write_async<'a>(&'a mut self, buf: &'a [u8]) -> Write<'a, Self> {
        Write::new(self, buf)
    }

    /// Write an entire buffer into this object.
    ///
    /// Creates a future that will write the entire contents of the buffer `buf` into
    /// this `AsyncWrite`.
    ///
    /// The returned future will not complete until all the data has been written.
    ///
    /// # Examples
    ///
    /// ```edition2018
    /// #![feature(async_await, await_macro, futures_api)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::AsyncWriteExt;
    /// use std::io::Cursor;
    ///
    /// let mut buf = [0u8; 5];
    /// let mut writer = Cursor::new(&mut buf[..]);
    ///
    /// await!(writer.write_all_async(&[1, 2, 3, 4])).unwrap();
    ///
    /// assert_eq!(writer.into_inner(), [1, 2, 3, 4, 0]);
    /// });
    /// ```
    fn write_all_async<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a, Self> {
        WriteAll::new(self, buf)
    }

    /// Creates a future which will entirely flush this `AsyncWrite`.
    ///
    /// # Examples
    ///
    /// ```edition2018
    /// #![feature(async_await, await_macro, futures_api)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::AsyncWriteExt;
    /// use std::io::{BufWriter, Cursor};
    ///
    /// let mut output = [0u8; 5];
    ///
    /// {
    ///     let mut writer = Cursor::new(&mut output[..]);
    ///     let mut buffered = BufWriter::new(writer);
    ///     await!(buffered.write_all_async(&[1, 2])).unwrap();
    ///     await!(buffered.write_all_async(&[3, 4])).unwrap();
    ///     await!(buffered.flush_async()).unwrap();
    /// }
    ///
    /// assert_eq!(output, [1, 2, 3, 4, 0]);
    /// });
    /// ```
    fn flush_async<'a>(&mut self) -> Flush<Self> {
        Flush::new(self)
    }
}

impl<T: AsyncRead + ?Sized> AsyncReadExt for T {}
impl<T: AsyncWrite + ?Sized> AsyncWriteExt for T {}
