use crate::io::util::flush::{flush, Flush};
use crate::io::util::shutdown::{shutdown, Shutdown};
use crate::io::util::write::{write, Write};
use crate::io::util::write_all::{write_all, WriteAll};
use crate::io::util::write_buf::{write_buf, WriteBuf};
use crate::io::util::write_int::{WriteI128, WriteI16, WriteI32, WriteI64, WriteI8};
use crate::io::util::write_int::{WriteU128, WriteU16, WriteU32, WriteU64, WriteU8};
use crate::io::AsyncWrite;

use bytes::Buf;

cfg_io_util! {
    /// Defines numeric writer
    macro_rules! write_impl {
        (
            $(
                $(#[$outer:meta])*
                fn $name:ident(&mut self, n: $ty:ty) -> $($fut:ident)*;
            )*
        ) => {
            $(
                $(#[$outer])*
                fn $name<'a>(&'a mut self, n: $ty) -> $($fut)*<&'a mut Self> where Self: Unpin {
                    $($fut)*::new(self, n)
                }
            )*
        }
    }

    /// Writes bytes to a sink.
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
        /// Writes a buffer into this writer, returning how many bytes were
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
        /// # Return
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
        ///     let mut file = File::create("foo.txt").await?;
        ///
        ///     // Writes some prefix of the byte string, not necessarily all of it.
        ///     file.write(b"some bytes").await?;
        ///     Ok(())
        /// }
        /// ```
        fn write<'a>(&'a mut self, src: &'a [u8]) -> Write<'a, Self>
        where
            Self: Unpin,
        {
            write(self, src)
        }

        /// Writes a buffer into this writer, advancing the buffer's internal
        /// cursor.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> io::Result<usize>;
        /// ```
        ///
        /// This function will attempt to write the entire contents of `buf`, but
        /// the entire write may not succeed, or the write may also generate an
        /// error. After the operation completes, the buffer's
        /// internal cursor is advanced by the number of bytes written. A
        /// subsequent call to `write_buf` using the **same** `buf` value will
        /// resume from the point that the first call to `write_buf` completed.
        /// A call to `write` represents *at most one* attempt to write to any
        /// wrapped object.
        ///
        /// # Return
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
        /// [`File`] implements `Read` and [`Cursor<&[u8]>`] implements [`Buf`]:
        ///
        /// [`File`]: crate::fs::File
        /// [`Buf`]: bytes::Buf
        ///
        /// ```no_run
        /// use tokio::io::{self, AsyncWriteExt};
        /// use tokio::fs::File;
        ///
        /// use bytes::Buf;
        /// use std::io::Cursor;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut file = File::create("foo.txt").await?;
        ///     let mut buffer = Cursor::new(b"data to write");
        ///
        ///     // Loop until the entire contents of the buffer are written to
        ///     // the file.
        ///     while buffer.has_remaining() {
        ///         // Writes some prefix of the byte string, not necessarily
        ///         // all of it.
        ///         file.write_buf(&mut buffer).await?;
        ///     }
        ///
        ///     Ok(())
        /// }
        /// ```
        fn write_buf<'a, B>(&'a mut self, src: &'a mut B) -> WriteBuf<'a, Self, B>
        where
            Self: Sized,
            B: Buf,
        {
            write_buf(self, src)
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

        write_impl! {
            /// Writes an unsigned 8-bit integer to the underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_u8(&mut self, n: u8) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write unsigned 8 bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_u8(2).await?;
            ///     writer.write_u8(5).await?;
            ///
            ///     assert_eq!(writer, b"\x02\x05");
            ///     Ok(())
            /// }
            /// ```
            fn write_u8(&mut self, n: u8) -> WriteU8;

            /// Writes an unsigned 8-bit integer to the underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_i8(&mut self, n: i8) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write unsigned 8 bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_u8(2).await?;
            ///     writer.write_u8(5).await?;
            ///
            ///     assert_eq!(writer, b"\x02\x05");
            ///     Ok(())
            /// }
            /// ```
            fn write_i8(&mut self, n: i8) -> WriteI8;

            /// Writes an unsigned 16-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_u16(&mut self, n: u16) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write unsigned 16-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_u16(517).await?;
            ///     writer.write_u16(768).await?;
            ///
            ///     assert_eq!(writer, b"\x02\x05\x03\x00");
            ///     Ok(())
            /// }
            /// ```
            fn write_u16(&mut self, n: u16) -> WriteU16;

            /// Writes a signed 16-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_i16(&mut self, n: i16) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write signed 16-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_i16(193).await?;
            ///     writer.write_i16(-132).await?;
            ///
            ///     assert_eq!(writer, b"\x00\xc1\xff\x7c");
            ///     Ok(())
            /// }
            /// ```
            fn write_i16(&mut self, n: i16) -> WriteI16;

            /// Writes an unsigned 32-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_u32(&mut self, n: u32) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write unsigned 32-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_u32(267).await?;
            ///     writer.write_u32(1205419366).await?;
            ///
            ///     assert_eq!(writer, b"\x00\x00\x01\x0b\x47\xd9\x3d\x66");
            ///     Ok(())
            /// }
            /// ```
            fn write_u32(&mut self, n: u32) -> WriteU32;

            /// Writes a signed 32-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_i32(&mut self, n: i32) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write signed 32-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_i32(267).await?;
            ///     writer.write_i32(1205419366).await?;
            ///
            ///     assert_eq!(writer, b"\x00\x00\x01\x0b\x47\xd9\x3d\x66");
            ///     Ok(())
            /// }
            /// ```
            fn write_i32(&mut self, n: i32) -> WriteI32;

            /// Writes an unsigned 64-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_u64(&mut self, n: u64) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write unsigned 64-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_u64(918733457491587).await?;
            ///     writer.write_u64(143).await?;
            ///
            ///     assert_eq!(writer, b"\x00\x03\x43\x95\x4d\x60\x86\x83\x00\x00\x00\x00\x00\x00\x00\x8f");
            ///     Ok(())
            /// }
            /// ```
            fn write_u64(&mut self, n: u64) -> WriteU64;

            /// Writes an signed 64-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_i64(&mut self, n: i64) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write signed 64-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_i64(i64::min_value()).await?;
            ///     writer.write_i64(i64::max_value()).await?;
            ///
            ///     assert_eq!(writer, b"\x80\x00\x00\x00\x00\x00\x00\x00\x7f\xff\xff\xff\xff\xff\xff\xff");
            ///     Ok(())
            /// }
            /// ```
            fn write_i64(&mut self, n: i64) -> WriteI64;

            /// Writes an unsigned 128-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_u128(&mut self, n: u128) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write unsigned 128-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_u128(16947640962301618749969007319746179).await?;
            ///
            ///     assert_eq!(writer, vec![
            ///         0x00, 0x03, 0x43, 0x95, 0x4d, 0x60, 0x86, 0x83,
            ///         0x00, 0x03, 0x43, 0x95, 0x4d, 0x60, 0x86, 0x83
            ///     ]);
            ///     Ok(())
            /// }
            /// ```
            fn write_u128(&mut self, n: u128) -> WriteU128;

            /// Writes an signed 128-bit integer in big-endian order to the
            /// underlying writer.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn write_i128(&mut self, n: i128) -> io::Result<()>;
            /// ```
            ///
            /// It is recommended to use a buffered writer to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncWriteExt::write_all`].
            ///
            /// [`AsyncWriteExt::write_all`]: AsyncWriteExt::write_all
            ///
            /// # Examples
            ///
            /// Write signed 128-bit integers to a `AsyncWrite`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncWriteExt};
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut writer = Vec::new();
            ///
            ///     writer.write_i128(i128::min_value()).await?;
            ///
            ///     assert_eq!(writer, vec![
            ///         0x80, 0, 0, 0, 0, 0, 0, 0,
            ///         0, 0, 0, 0, 0, 0, 0, 0
            ///     ]);
            ///     Ok(())
            /// }
            /// ```
            fn write_i128(&mut self, n: i128) -> WriteI128;
        }

        /// Flushes this output stream, ensuring that all intermediately buffered
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
