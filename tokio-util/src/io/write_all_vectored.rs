use tokio::io::AsyncWrite;

use pin_project_lite::pin_project;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::{future::Future, io::IoSlice};
use std::{io, mem};

pin_project! {
    /// A future that writes all data from multiple buffers to a writer.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct WriteAllVectored<'a, 'b, W: ?Sized> {
        writer: &'a mut W,
        bufs: &'a mut [IoSlice<'b>],
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}
/// Like [`write_all`] but writes all data from multiple buffers into this writer.
///
/// This function writes multiple (possibly non-contiguous) buffers into the writer,
/// using the `writev` syscall to potentially write in a single system call.
///
/// Interrupted writes are retried. Other errors are returned immediately.
///
/// Equivalent to:
///
/// ```ignore
/// async fn write_all_vectored<W: AsyncWrite + Unpin + ?Sized>(
///     writer: &mut W,
///     mut bufs: &mut [IoSlice<'_>]
/// ) -> io::Result<()> {
///     while !bufs.is_empty() {
///         let n = match write_vectored(writer, bufs).await {
///             Ok(n) => n,
///             Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
///             Err(e) => return Err(e),
///         };
///         if n == 0 {
///             return Err(io::ErrorKind::WriteZero.into());
///         }
///         IoSlice::advance_slices(&mut bufs, n);
///     }
///     Ok(())
/// }
/// ```
///
/// # Cancel safety
///
/// This method is not cancellation safe. If it is used as the event
/// in a `tokio::select!` statement and some other
/// branch completes first, then the provided buffer may have been
/// partially written, but future calls to `write_all_vectored` will
/// have lost its place in the buffer.
///
/// # Examples
///
/// ```rust
/// use tokio_util::io::write_all_vectored;
/// use std::io::IoSlice;
///
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() -> std::io::Result<()> {
///
///     let mut writer = Vec::new();
///     let bufs = &mut [
///         IoSlice::new(&[1]),
///         IoSlice::new(&[2, 3]),
///         IoSlice::new(&[4, 5, 6]),
///     ];
///
///     write_all_vectored(&mut writer, bufs).await?;
///
///     // Note: `bufs` has been modified by `IoSlice::advance_slices` and should not be reused.
///     assert_eq!(writer, &[1, 2, 3, 4, 5, 6]);
///     Ok(())
/// }
/// ```
///
/// # Notes
///
/// See the documentation for [`Write::write_all_vectored`] from std.
/// After calling this function, the buffer slices may have
/// been advanced and should not be reused.
///
/// [`Write::write_all_vectored`]: std::io::Write::write_all_vectored
/// [`write_all`]: tokio::io::AsyncWriteExt::write_all
/// [`writev`]: https://man7.org/linux/man-pages/man3/writev.3p.html
pub fn write_all_vectored<'a, 'b, W>(
    writer: &'a mut W,
    bufs: &'a mut [IoSlice<'b>],
) -> WriteAllVectored<'a, 'b, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    WriteAllVectored {
        writer,
        bufs,
        _pin: PhantomPinned,
    }
}

impl<W> Future for WriteAllVectored<'_, '_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = self.project();
        while !me.bufs.is_empty() {
            // advance to first non-empty buffer
            let non_empty = match me.bufs.iter().position(|b| !b.is_empty()) {
                Some(pos) => pos,
                None => return Poll::Ready(Ok(())),
            };

            // drop empty buffers at the start
            *me.bufs = &mut mem::take(me.bufs)[non_empty..];

            let n = match ready!(Pin::new(&mut *me.writer).poll_write_vectored(cx, me.bufs)) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                res => res?,
            };
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            IoSlice::advance_slices(me.bufs, n);
        }

        Poll::Ready(Ok(()))
    }
}
