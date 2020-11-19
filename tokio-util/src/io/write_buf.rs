use bytes::Buf;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

/// Write data from an implementer of the [`Buf`] trait to an [`AsyncWrite`],
/// advancing the buffer's internal cursor.
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
/// This function will use [vectored writes] when the [`AsyncWrite`] supports
/// vectored writes.
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
/// [`File`] implements [`AsyncWrite`] and [`Cursor<&[u8]>`] implements [`Buf`]:
///
/// ```no_run
/// use tokio::io;
/// use tokio::fs::File;
/// use tokio_util::io::write_buf;
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
///         write_buf(&mut file, &mut buffer).await?;
///     }
///
///     Ok(())
/// }
/// ```
///
/// [`Buf`]: bytes::Buf
/// [`AsyncWrite`]: tokio::io::AsyncWrite
/// [`File`]: tokio::fs::File
/// [vectored writes]: tokio::io::AsyncWrite::poll_write_vectored
pub async fn write_buf<W, B>(write: &mut W, buf: &mut B) -> io::Result<usize>
where
    W: AsyncWrite + Unpin,
    B: Buf,
{
    return WriteBufFn(write, buf).await;

    struct WriteBufFn<'a, W, B>(&'a mut W, &'a mut B);

    impl<'a, W, B> Future for WriteBufFn<'a, W, B>
    where
        W: AsyncWrite + Unpin,
        B: Buf,
    {
        type Output = io::Result<usize>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = &mut *self;
            crate::util::poll_write_buf(Pin::new(this.0), cx, this.1)
        }
    }
}
