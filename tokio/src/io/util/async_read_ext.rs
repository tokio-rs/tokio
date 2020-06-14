use crate::io::util::chain::{chain, Chain};
use crate::io::util::read::{read, Read};
use crate::io::util::read_buf::{read_buf, ReadBuf};
use crate::io::util::read_exact::{read_exact, ReadExact};
use crate::io::util::read_int::{ReadI128, ReadI16, ReadI32, ReadI64, ReadI8};
use crate::io::util::read_int::{ReadU128, ReadU16, ReadU32, ReadU64, ReadU8};
use crate::io::util::read_to_end::{read_to_end, ReadToEnd};
use crate::io::util::read_to_string::{read_to_string, ReadToString};
use crate::io::util::take::{take, Take};
use crate::io::AsyncRead;

use bytes::BufMut;

cfg_io_util! {
    /// Defines numeric reader
    macro_rules! read_impl {
        (
            $(
                $(#[$outer:meta])*
                fn $name:ident(&mut self) -> $($fut:ident)*;
            )*
        ) => {
            $(
                $(#[$outer])*
                fn $name<'a>(&'a mut self) -> $($fut)*<&'a mut Self> where Self: Unpin {
                    $($fut)*::new(self)
                }
            )*
        }
    }

    /// Reads bytes from a source.
    ///
    /// Implemented as an extention trait, adding utility methods to all
    /// [`AsyncRead`] types. Callers will tend to import this trait instead of
    /// [`AsyncRead`].
    ///
    /// As a convenience, this trait may be imported using the [`prelude`]:
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::prelude::*;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut f = File::open("foo.txt").await?;
    ///     let mut buffer = [0; 10];
    ///
    ///     // The `read` method is defined by this trait.
    ///     let n = f.read(&mut buffer[..]).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// See [module][crate::io] documentation for more details.
    ///
    /// [`AsyncRead`]: AsyncRead
    /// [`prelude`]: crate::prelude
    pub trait AsyncReadExt: AsyncRead {
        /// Creates a new `AsyncRead` instance that chains this stream with
        /// `next`.
        ///
        /// The returned `AsyncRead` instance will first read all bytes from this object
        /// until EOF is encountered. Afterwards the output is equivalent to the
        /// output of `next`.
        ///
        /// # Examples
        ///
        /// [`File`][crate::fs::File]s implement `AsyncRead`:
        ///
        /// ```no_run
        /// use tokio::fs::File;
        /// use tokio::io::{self, AsyncReadExt};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let f1 = File::open("foo.txt").await?;
        ///     let f2 = File::open("bar.txt").await?;
        ///
        ///     let mut handle = f1.chain(f2);
        ///     let mut buffer = String::new();
        ///
        ///     // read the value into a String. We could use any AsyncRead
        ///     // method here, this is just one example.
        ///     handle.read_to_string(&mut buffer).await?;
        ///     Ok(())
        /// }
        /// ```
        fn chain<R>(self, next: R) -> Chain<Self, R>
        where
            Self: Sized,
            R: AsyncRead,
        {
            chain(self, next)
        }

        /// Pulls some bytes from this source into the specified buffer,
        /// returning how many bytes were read.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
        /// ```
        ///
        /// This function does not provide any guarantees about whether it
        /// completes immediately or asynchronously
        ///
        /// If the return value of this method is `Ok(n)`, then it must be
        /// guaranteed that `0 <= n <= buf.len()`. A nonzero `n` value indicates
        /// that the buffer `buf` has been filled in with `n` bytes of data from
        /// this source. If `n` is `0`, then it can indicate one of two
        /// scenarios:
        ///
        /// 1. This reader has reached its "end of file" and will likely no longer
        ///    be able to produce bytes. Note that this does not mean that the
        ///    reader will *always* no longer be able to produce bytes.
        /// 2. The buffer specified was 0 bytes in length.
        ///
        /// No guarantees are provided about the contents of `buf` when this
        /// function is called, implementations cannot rely on any property of the
        /// contents of `buf` being `true`. It is recommended that *implementations*
        /// only write data to `buf` instead of reading its contents.
        ///
        /// Correspondingly, however, *callers* of this method may not assume
        /// any guarantees about how the implementation uses `buf`. It is
        /// possible that the code that's supposed to write to the buffer might
        /// also read from it. It is your responsibility to make sure that `buf`
        /// is initialized before calling `read`.
        ///
        /// # Errors
        ///
        /// If this function encounters any form of I/O or other error, an error
        /// variant will be returned. If an error is returned then it must be
        /// guaranteed that no bytes were read.
        ///
        /// # Examples
        ///
        /// [`File`][crate::fs::File]s implement `Read`:
        ///
        /// ```no_run
        /// use tokio::fs::File;
        /// use tokio::io::{self, AsyncReadExt};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut f = File::open("foo.txt").await?;
        ///     let mut buffer = [0; 10];
        ///
        ///     // read up to 10 bytes
        ///     let n = f.read(&mut buffer[..]).await?;
        ///
        ///     println!("The bytes: {:?}", &buffer[..n]);
        ///     Ok(())
        /// }
        /// ```
        fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> Read<'a, Self>
        where
            Self: Unpin,
        {
            read(self, buf)
        }

        /// Pulls some bytes from this source into the specified buffer,
        /// advancing the buffer's internal cursor.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> io::Result<usize>;
        /// ```
        ///
        /// Usually, only a single `read` syscall is issued, even if there is
        /// more space in the supplied buffer.
        ///
        /// This function does not provide any guarantees about whether it
        /// completes immediately or asynchronously
        ///
        /// # Return
        ///
        /// On a successful read, the number of read bytes is returned. If the
        /// supplied buffer is not empty and the function returns `Ok(0)` then
        /// the source as reached an "end-of-file" event.
        ///
        /// # Errors
        ///
        /// If this function encounters any form of I/O or other error, an error
        /// variant will be returned. If an error is returned then it must be
        /// guaranteed that no bytes were read.
        ///
        /// # Examples
        ///
        /// [`File`] implements `Read` and [`BytesMut`] implements [`BufMut`]:
        ///
        /// [`File`]: crate::fs::File
        /// [`BytesMut`]: bytes::BytesMut
        /// [`BufMut`]: bytes::BufMut
        ///
        /// ```no_run
        /// use tokio::fs::File;
        /// use tokio::io::{self, AsyncReadExt};
        ///
        /// use bytes::BytesMut;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut f = File::open("foo.txt").await?;
        ///     let mut buffer = BytesMut::with_capacity(10);
        ///
        ///     assert!(buffer.is_empty());
        ///
        ///     // read up to 10 bytes, note that the return value is not needed
        ///     // to access the data that was read as `buffer`'s internal
        ///     // cursor is updated.
        ///     f.read_buf(&mut buffer).await?;
        ///
        ///     println!("The bytes: {:?}", &buffer[..]);
        ///     Ok(())
        /// }
        /// ```
        fn read_buf<'a, B>(&'a mut self, buf: &'a mut B) -> ReadBuf<'a, Self, B>
        where
            Self: Sized + Unpin,
            B: BufMut,
        {
            read_buf(self, buf)
        }

        /// Reads the exact number of bytes required to fill `buf`.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<usize>;
        /// ```
        ///
        /// This function reads as many bytes as necessary to completely fill
        /// the specified buffer `buf`.
        ///
        /// # Errors
        ///
        /// If the operation encounters an "end of file" before completely
        /// filling the buffer, it returns an error of the kind
        /// [`ErrorKind::UnexpectedEof`]. The contents of `buf` are unspecified
        /// in this case.
        ///
        /// If any other read error is encountered then the operation
        /// immediately returns. The contents of `buf` are unspecified in this
        /// case.
        ///
        /// If this operation returns an error, it is unspecified how many bytes
        /// it has read, but it will never read more than would be necessary to
        /// completely fill the buffer.
        ///
        /// # Examples
        ///
        /// [`File`][crate::fs::File]s implement `Read`:
        ///
        /// ```no_run
        /// use tokio::fs::File;
        /// use tokio::io::{self, AsyncReadExt};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut f = File::open("foo.txt").await?;
        ///     let mut buffer = [0; 10];
        ///
        ///     // read exactly 10 bytes
        ///     f.read_exact(&mut buffer).await?;
        ///     Ok(())
        /// }
        /// ```
        ///
        /// [`ErrorKind::UnexpectedEof`]: std::io::ErrorKind::UnexpectedEof
        fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a, Self>
        where
            Self: Unpin,
        {
            read_exact(self, buf)
        }

        read_impl! {
            /// Reads an unsigned 8 bit integer from the underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_u8(&mut self) -> io::Result<u8>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read unsigned 8 bit integers from an `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![2, 5]);
            ///
            ///     assert_eq!(2, reader.read_u8().await?);
            ///     assert_eq!(5, reader.read_u8().await?);
            ///
            ///     Ok(())
            /// }
            /// ```
            fn read_u8(&mut self) -> ReadU8;

            /// Reads a signed 8 bit integer from the underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_i8(&mut self) -> io::Result<i8>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read unsigned 8 bit integers from an `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![0x02, 0xfb]);
            ///
            ///     assert_eq!(2, reader.read_i8().await?);
            ///     assert_eq!(-5, reader.read_i8().await?);
            ///
            ///     Ok(())
            /// }
            /// ```
            fn read_i8(&mut self) -> ReadI8;

            /// Reads an unsigned 16-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_u16(&mut self) -> io::Result<u16>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read unsigned 16 bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![2, 5, 3, 0]);
            ///
            ///     assert_eq!(517, reader.read_u16().await?);
            ///     assert_eq!(768, reader.read_u16().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_u16(&mut self) -> ReadU16;

            /// Reads a signed 16-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_i16(&mut self) -> io::Result<i16>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read signed 16 bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![0x00, 0xc1, 0xff, 0x7c]);
            ///
            ///     assert_eq!(193, reader.read_i16().await?);
            ///     assert_eq!(-132, reader.read_i16().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_i16(&mut self) -> ReadI16;

            /// Reads an unsigned 32-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_u32(&mut self) -> io::Result<u32>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read unsigned 32-bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![0x00, 0x00, 0x01, 0x0b]);
            ///
            ///     assert_eq!(267, reader.read_u32().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_u32(&mut self) -> ReadU32;

            /// Reads a signed 32-bit integer in big-endian order from the
            /// underlying reader.
            ///
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_i32(&mut self) -> io::Result<i32>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read signed 32-bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![0xff, 0xff, 0x7a, 0x33]);
            ///
            ///     assert_eq!(-34253, reader.read_i32().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_i32(&mut self) -> ReadI32;

            /// Reads an unsigned 64-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_u64(&mut self) -> io::Result<u64>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read unsigned 64-bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![
            ///         0x00, 0x03, 0x43, 0x95, 0x4d, 0x60, 0x86, 0x83
            ///     ]);
            ///
            ///     assert_eq!(918733457491587, reader.read_u64().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_u64(&mut self) -> ReadU64;

            /// Reads an signed 64-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_i64(&mut self) -> io::Result<i64>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read signed 64-bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![0x80, 0, 0, 0, 0, 0, 0, 0]);
            ///
            ///     assert_eq!(i64::min_value(), reader.read_i64().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_i64(&mut self) -> ReadI64;

            /// Reads an unsigned 128-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_u128(&mut self) -> io::Result<u128>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read unsigned 128-bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![
            ///         0x00, 0x03, 0x43, 0x95, 0x4d, 0x60, 0x86, 0x83,
            ///         0x00, 0x03, 0x43, 0x95, 0x4d, 0x60, 0x86, 0x83
            ///     ]);
            ///
            ///     assert_eq!(16947640962301618749969007319746179, reader.read_u128().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_u128(&mut self) -> ReadU128;

            /// Reads an signed 128-bit integer in big-endian order from the
            /// underlying reader.
            ///
            /// Equivalent to:
            ///
            /// ```ignore
            /// async fn read_i128(&mut self) -> io::Result<i128>;
            /// ```
            ///
            /// It is recommended to use a buffered reader to avoid excessive
            /// syscalls.
            ///
            /// # Errors
            ///
            /// This method returns the same errors as [`AsyncReadExt::read_exact`].
            ///
            /// [`AsyncReadExt::read_exact`]: AsyncReadExt::read_exact
            ///
            /// # Examples
            ///
            /// Read signed 128-bit big-endian integers from a `AsyncRead`:
            ///
            /// ```rust
            /// use tokio::io::{self, AsyncReadExt};
            ///
            /// use std::io::Cursor;
            ///
            /// #[tokio::main]
            /// async fn main() -> io::Result<()> {
            ///     let mut reader = Cursor::new(vec![
            ///         0x80, 0, 0, 0, 0, 0, 0, 0,
            ///         0, 0, 0, 0, 0, 0, 0, 0
            ///     ]);
            ///
            ///     assert_eq!(i128::min_value(), reader.read_i128().await?);
            ///     Ok(())
            /// }
            /// ```
            fn read_i128(&mut self) -> ReadI128;
        }

        /// Reads all bytes until EOF in this source, placing them into `buf`.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize>;
        /// ```
        ///
        /// All bytes read from this source will be appended to the specified
        /// buffer `buf`. This function will continuously call [`read()`] to
        /// append more data to `buf` until [`read()`][read] returns `Ok(0)`.
        ///
        /// If successful, the total number of bytes read is returned.
        ///
        /// # Errors
        ///
        /// If a read error is encountered then the `read_to_end` operation
        /// immediately completes. Any bytes which have already been read will
        /// be appended to `buf`.
        ///
        /// # Examples
        ///
        /// [`File`][crate::fs::File]s implement `Read`:
        ///
        /// ```no_run
        /// use tokio::io::{self, AsyncReadExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut f = File::open("foo.txt").await?;
        ///     let mut buffer = Vec::new();
        ///
        ///     // read the whole file
        ///     f.read_to_end(&mut buffer).await?;
        ///     Ok(())
        /// }
        /// ```
        ///
        /// (See also the [`tokio::fs::read`] convenience function for reading from a
        /// file.)
        ///
        /// [`tokio::fs::read`]: crate::fs::read::read
        fn read_to_end<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadToEnd<'a, Self>
        where
            Self: Unpin,
        {
            read_to_end(self, buf)
        }

        /// Reads all bytes until EOF in this source, appending them to `buf`.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize>;
        /// ```
        ///
        /// If successful, the number of bytes which were read and appended to
        /// `buf` is returned.
        ///
        /// # Errors
        ///
        /// If the data in this stream is *not* valid UTF-8 then an error is
        /// returned and `buf` is unchanged.
        ///
        /// See [`read_to_end`][AsyncReadExt::read_to_end] for other error semantics.
        ///
        /// # Examples
        ///
        /// [`File`][crate::fs::File]s implement `Read`:
        ///
        /// ```no_run
        /// use tokio::io::{self, AsyncReadExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut f = File::open("foo.txt").await?;
        ///     let mut buffer = String::new();
        ///
        ///     f.read_to_string(&mut buffer).await?;
        ///     Ok(())
        /// }
        /// ```
        ///
        /// (See also the [`crate::fs::read_to_string`] convenience function for
        /// reading from a file.)
        ///
        /// [`crate::fs::read_to_string`]: crate::fs::read_to_string::read_to_string
        fn read_to_string<'a>(&'a mut self, dst: &'a mut String) -> ReadToString<'a, Self>
        where
            Self: Unpin,
        {
            read_to_string(self, dst)
        }

        /// Creates an adaptor which reads at most `limit` bytes from it.
        ///
        /// This function returns a new instance of `AsyncRead` which will read
        /// at most `limit` bytes, after which it will always return EOF
        /// (`Ok(0)`). Any read errors will not count towards the number of
        /// bytes read and future calls to [`read()`][read] may succeed.
        ///
        /// # Examples
        ///
        /// [`File`][crate::fs::File]s implement `Read`:
        ///
        /// ```no_run
        /// use tokio::io::{self, AsyncReadExt};
        /// use tokio::fs::File;
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let f = File::open("foo.txt").await?;
        ///     let mut buffer = [0; 5];
        ///
        ///     // read at most five bytes
        ///     let mut handle = f.take(5);
        ///
        ///     handle.read(&mut buffer).await?;
        ///     Ok(())
        /// }
        /// ```
        fn take(self, limit: u64) -> Take<Self>
        where
            Self: Sized,
        {
            take(self, limit)
        }
    }
}

impl<R: AsyncRead + ?Sized> AsyncReadExt for R {}
