use std::io::{self, IoSlice};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::flush::{flush, Flush};
use super::shutdown::{shutdown, Shutdown};
use super::write::{write, Write};
use super::write_all::{write_all, WriteAll};
use super::write_buf::{write_buf, WriteBuf};
use super::write_int::{
    WriteI128, WriteI128Le, WriteI16, WriteI16Le, WriteI32, WriteI32Le, WriteI64, WriteI64Le,
    WriteI8,
};
use super::write_int::{
    WriteU128, WriteU128Le, WriteU16, WriteU16Le, WriteU32, WriteU32Le, WriteU64, WriteU64Le,
    WriteU8,
};

use bytes::Buf;

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
            fn $name<'a>(&'a mut self, n: $ty) -> $($fut)*<&'a mut Self> where Self: Unpin + Sized {
                $($fut)*::new(self, n)
            }
        )*
    }
}

/// Writes bytes asynchronously.
///
/// The trait inherits from [`std::io::Write`] and indicates that an I/O object is
/// **nonblocking**. All non-blocking I/O objects must return an error when
/// bytes cannot be written instead of blocking the current thread.
///
/// Specifically, this means that the [`poll_write`] function will return one of
/// the following:
///
/// * `Poll::Ready(Ok(n))` means that `n` bytes of data was immediately
///   written.
///
/// * `Poll::Pending` means that no data was written from the buffer
///   provided. The I/O object is not currently writable but may become writable
///   in the future. Most importantly, **the current future's task is scheduled
///   to get unparked when the object is writable**. This means that like
///   `Future::poll` you'll receive a notification when the I/O object is
///   writable again.
///
/// * `Poll::Ready(Err(e))` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the [`write`][stdwrite] method only works in
/// the context of a future's task. The object may panic if used outside of a task.
///
/// Note that this trait also represents that the  [`Write::flush`][stdflush] method
/// works very similarly to the `write` method, notably that `Ok(())` means that the
/// writer has successfully been flushed, a "would block" error means that the
/// current task is ready to receive a notification when flushing can make more
/// progress, and otherwise normal errors can happen as well.
///
/// Utilities for working with `AsyncWrite` values are provided by
/// [`AsyncWrite`].
///
/// [`std::io::Write`]: std::io::Write
/// [`poll_write`]: AsyncWrite::poll_write()
/// [stdwrite]: std::io::Write::write()
/// [stdflush]: std::io::Write::flush()
/// [`AsyncWrite`]: crate::io::AsyncWrite
pub trait AsyncWrite {
    /// Attempt to write bytes from `buf` into the object.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`.
    ///
    /// If the object is not ready for writing, the method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker()`) to receive a notification when the object becomes
    /// writable or is closed.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>>;

    /// Attempts to flush the object, ensuring that any buffered data reach
    /// their destination.
    ///
    /// On success, returns `Poll::Ready(Ok(()))`.
    ///
    /// If flushing cannot immediately complete, this method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker()`) to receive a notification when the object can make
    /// progress towards flushing.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>>;

    /// Initiates or attempts to shut down this writer, returning success when
    /// the I/O connection has completely shut down.
    ///
    /// This method is intended to be used for asynchronous shutdown of I/O
    /// connections. For example this is suitable for implementing shutdown of a
    /// TLS connection or calling `TcpStream::shutdown` on a proxied connection.
    /// Protocols sometimes need to flush out final pieces of data or otherwise
    /// perform a graceful shutdown handshake, reading/writing more data as
    /// appropriate. This method is the hook for such protocols to implement the
    /// graceful shutdown logic.
    ///
    /// This `shutdown` method is required by implementers of the
    /// `AsyncWrite` trait. Wrappers typically just want to proxy this call
    /// through to the wrapped type, and base types will typically implement
    /// shutdown logic here or just return `Ok(().into())`. Note that if you're
    /// wrapping an underlying `AsyncWrite` a call to `shutdown` implies that
    /// transitively the entire stream has been shut down. After your wrapper's
    /// shutdown logic has been executed you should shut down the underlying
    /// stream.
    ///
    /// Invocation of a `shutdown` implies an invocation of `flush`. Once this
    /// method returns `Ready` it implies that a flush successfully happened
    /// before the shutdown happened. That is, callers don't need to call
    /// `flush` before calling `shutdown`. They can rely that by calling
    /// `shutdown` any pending buffered data will be written out.
    ///
    /// # Return value
    ///
    /// This function returns a `Poll<io::Result<()>>` classified as such:
    ///
    /// * `Poll::Ready(Ok(()))` - indicates that the connection was
    ///   successfully shut down and is now safe to deallocate/drop/close
    ///   resources associated with it. This method means that the current task
    ///   will no longer receive any notifications due to this method and the
    ///   I/O object itself is likely no longer usable.
    ///
    /// * `Poll::Pending` - indicates that shutdown is initiated but could
    ///   not complete just yet. This may mean that more I/O needs to happen to
    ///   continue this shutdown operation. The current task is scheduled to
    ///   receive a notification when it's otherwise ready to continue the
    ///   shutdown operation. When woken up this method should be called again.
    ///
    /// * `Poll::Ready(Err(e))` - indicates a fatal error has happened with shutdown,
    ///   indicating that the shutdown operation did not complete successfully.
    ///   This typically means that the I/O object is no longer usable.
    ///
    /// # Errors
    ///
    /// This function can return normal I/O errors through `Err`, described
    /// above. Additionally this method may also render the underlying
    /// `Write::write` method no longer usable (e.g. will return errors in the
    /// future). It's recommended that once `shutdown` is called the
    /// `write` method is no longer called.
    ///
    /// # Panics
    ///
    /// This function will panic if not called within the context of a future's
    /// task.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>>;

    /// Like [`poll_write`], except that it writes from a slice of buffers.
    ///
    /// Data is copied from each buffer in order, with the final buffer
    /// read from possibly being only partially consumed. This method must
    /// behave as a call to [`write`] with the buffers concatenated would.
    ///
    /// The default implementation calls [`poll_write`] with either the first nonempty
    /// buffer provided, or an empty one if none exists.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`.
    ///
    /// If the object is not ready for writing, the method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker()`) to receive a notification when the object becomes
    /// writable or is closed.
    ///
    /// # Note
    ///
    /// This should be implemented as a single "atomic" write action. If any
    /// data has been partially written, it is wrong to return an error or
    /// pending.
    ///
    /// [`poll_write`]: AsyncWrite::poll_write
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        let buf = bufs
            .iter()
            .find(|b| !b.is_empty())
            .map_or(&[][..], |b| &**b);
        self.poll_write(cx, buf)
    }

    /// Determines if this writer has an efficient [`poll_write_vectored`]
    /// implementation.
    ///
    /// If a writer does not override the default [`poll_write_vectored`]
    /// implementation, code using it may want to avoid the method all together
    /// and coalesce writes into a single buffer for higher performance.
    ///
    /// The default implementation returns `false`.
    ///
    /// [`poll_write_vectored`]: AsyncWrite::poll_write_vectored
    fn is_write_vectored(&self) -> bool {
        false
    }

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
    /// use tokio::io::{self, AsyncWrite};
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
        Self: Sized + Unpin,
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
    /// A call to `write_buf` represents *at most one* attempt to write to any
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
    /// use tokio::io::{self, AsyncWrite};
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
        Self: Sized + Unpin,
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
    /// use tokio::io::{self, AsyncWrite};
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
    /// [`write`]: AsyncWrite::write
    fn write_all<'a>(&'a mut self, src: &'a [u8]) -> WriteAll<'a, Self>
    where
        Self: Sized + Unpin,
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 8 bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 8 bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 16-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 16-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 32-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 32-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 64-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 64-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 128-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 128-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
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


        /// Writes an unsigned 16-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_u16_le(&mut self, n: u16) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 16-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_u16_le(517).await?;
        ///     writer.write_u16_le(768).await?;
        ///
        ///     assert_eq!(writer, b"\x05\x02\x00\x03");
        ///     Ok(())
        /// }
        /// ```
        fn write_u16_le(&mut self, n: u16) -> WriteU16Le;

        /// Writes a signed 16-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_i16_le(&mut self, n: i16) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 16-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_i16_le(193).await?;
        ///     writer.write_i16_le(-132).await?;
        ///
        ///     assert_eq!(writer, b"\xc1\x00\x7c\xff");
        ///     Ok(())
        /// }
        /// ```
        fn write_i16_le(&mut self, n: i16) -> WriteI16Le;

        /// Writes an unsigned 32-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_u32_le(&mut self, n: u32) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 32-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_u32_le(267).await?;
        ///     writer.write_u32_le(1205419366).await?;
        ///
        ///     assert_eq!(writer, b"\x0b\x01\x00\x00\x66\x3d\xd9\x47");
        ///     Ok(())
        /// }
        /// ```
        fn write_u32_le(&mut self, n: u32) -> WriteU32Le;

        /// Writes a signed 32-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_i32_le(&mut self, n: i32) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 32-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_i32_le(267).await?;
        ///     writer.write_i32_le(1205419366).await?;
        ///
        ///     assert_eq!(writer, b"\x0b\x01\x00\x00\x66\x3d\xd9\x47");
        ///     Ok(())
        /// }
        /// ```
        fn write_i32_le(&mut self, n: i32) -> WriteI32Le;

        /// Writes an unsigned 64-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_u64_le(&mut self, n: u64) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 64-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_u64_le(918733457491587).await?;
        ///     writer.write_u64_le(143).await?;
        ///
        ///     assert_eq!(writer, b"\x83\x86\x60\x4d\x95\x43\x03\x00\x8f\x00\x00\x00\x00\x00\x00\x00");
        ///     Ok(())
        /// }
        /// ```
        fn write_u64_le(&mut self, n: u64) -> WriteU64Le;

        /// Writes an signed 64-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_i64_le(&mut self, n: i64) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 64-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_i64_le(i64::min_value()).await?;
        ///     writer.write_i64_le(i64::max_value()).await?;
        ///
        ///     assert_eq!(writer, b"\x00\x00\x00\x00\x00\x00\x00\x80\xff\xff\xff\xff\xff\xff\xff\x7f");
        ///     Ok(())
        /// }
        /// ```
        fn write_i64_le(&mut self, n: i64) -> WriteI64Le;

        /// Writes an unsigned 128-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_u128_le(&mut self, n: u128) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write unsigned 128-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_u128_le(16947640962301618749969007319746179).await?;
        ///
        ///     assert_eq!(writer, vec![
        ///         0x83, 0x86, 0x60, 0x4d, 0x95, 0x43, 0x03, 0x00,
        ///         0x83, 0x86, 0x60, 0x4d, 0x95, 0x43, 0x03, 0x00,
        ///     ]);
        ///     Ok(())
        /// }
        /// ```
        fn write_u128_le(&mut self, n: u128) -> WriteU128Le;

        /// Writes an signed 128-bit integer in little-endian order to the
        /// underlying writer.
        ///
        /// Equivalent to:
        ///
        /// ```ignore
        /// async fn write_i128_le(&mut self, n: i128) -> io::Result<()>;
        /// ```
        ///
        /// It is recommended to use a buffered writer to avoid excessive
        /// syscalls.
        ///
        /// # Errors
        ///
        /// This method returns the same errors as [`AsyncWrite::write_all`].
        ///
        /// [`AsyncWrite::write_all`]: AsyncWrite::write_all
        ///
        /// # Examples
        ///
        /// Write signed 128-bit integers to a `AsyncWrite`:
        ///
        /// ```rust
        /// use tokio::io::{self, AsyncWrite};
        ///
        /// #[tokio::main]
        /// async fn main() -> io::Result<()> {
        ///     let mut writer = Vec::new();
        ///
        ///     writer.write_i128_le(i128::min_value()).await?;
        ///
        ///     assert_eq!(writer, vec![
        ///          0, 0, 0, 0, 0, 0, 0,
        ///         0, 0, 0, 0, 0, 0, 0, 0, 0x80
        ///     ]);
        ///     Ok(())
        /// }
        /// ```
        fn write_i128_le(&mut self, n: i128) -> WriteI128Le;
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
    /// use tokio::io::{self, BufWriter, AsyncWrite};
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
        Self: Sized + Unpin,
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
    /// [`flush`]: fn@crate::io::AsyncWrite::flush
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::io::{self, BufWriter, AsyncWrite};
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
        Self: Sized + Unpin,
    {
        shutdown(self)
    }
}

macro_rules! deref_async_write {
    () => {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut **self).poll_write(cx, buf)
        }

        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut **self).poll_write_vectored(cx, bufs)
        }

        fn is_write_vectored(&self) -> bool {
            (**self).is_write_vectored()
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut **self).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut **self).poll_shutdown(cx)
        }
    };
}

impl<T: ?Sized + AsyncWrite + Unpin> AsyncWrite for Box<T> {
    deref_async_write!();
}

impl<T: ?Sized + AsyncWrite + Unpin> AsyncWrite for &mut T {
    deref_async_write!();
}

impl<P> AsyncWrite for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().as_mut().poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().as_mut().poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        (**self).is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().as_mut().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().as_mut().poll_shutdown(cx)
    }
}

impl AsyncWrite for Vec<u8> {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write_vectored(&mut *self, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for io::Cursor<&mut [u8]> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write(&mut *self, buf))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write_vectored(&mut *self, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(io::Write::flush(&mut *self))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncWrite for io::Cursor<&mut Vec<u8>> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write(&mut *self, buf))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write_vectored(&mut *self, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(io::Write::flush(&mut *self))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncWrite for io::Cursor<Vec<u8>> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write(&mut *self, buf))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write_vectored(&mut *self, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(io::Write::flush(&mut *self))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncWrite for io::Cursor<Box<[u8]>> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write(&mut *self, buf))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Write::write_vectored(&mut *self, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(io::Write::flush(&mut *self))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}
