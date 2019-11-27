use crate::io::util::chain::{chain, Chain};
use crate::io::util::read::{read, Read};
use crate::io::util::read_exact::{read_exact, ReadExact};
use crate::io::util::read_to_end::{read_to_end, ReadToEnd};
use crate::io::util::read_to_string::{read_to_string, ReadToString};
use crate::io::util::take::{take, Take};
use crate::io::AsyncRead;

cfg_io_util! {
    /// Read bytes from a source.
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
        /// Create a new `AsyncRead` instance that chains this stream with
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

        /// Pull some bytes from this source into the specified buffer,
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
        /// contents of `buf` being true. It is recommended that *implementations*
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

        /// Read the exact number of bytes required to fill `buf`.
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
        /// No guarantees are provided about the contents of `buf` when this
        /// function is called, implementations cannot rely on any property of
        /// the contents of `buf` being true. It is recommended that
        /// implementations only write data to `buf` instead of reading its
        /// contents.
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

        /// Read all bytes until EOF in this source, placing them into `buf`.
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

        /// Read all bytes until EOF in this source, appending them to `buf`.
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
