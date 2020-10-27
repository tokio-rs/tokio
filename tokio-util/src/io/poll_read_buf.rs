use bytes::BufMut;
use futures_core::ready;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

/// Try to read data from an `AsyncRead` into an implementer of the [`Buf`] trait.
///
/// [`Buf`]: bytes::Buf
///
/// # Example
///
/// ```
/// use bytes::{Bytes, BytesMut};
/// use tokio::stream;
/// use tokio::io::Result;
/// use tokio_util::io::{StreamReader, poll_read_buf};
/// use futures::future::poll_fn;
/// use std::pin::Pin;
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
///
/// // Create a reader from an iterator. This particular reader will always be
/// // ready.
/// let mut read = StreamReader::new(stream::iter(vec![Result::Ok(Bytes::from_static(&[0, 1, 2, 3]))]));
///
/// let mut buf = BytesMut::new();
/// let mut reads = 0;
///
/// loop {
///     reads += 1;
///     let n = poll_fn(|cx| poll_read_buf(Pin::new(&mut read), cx, &mut buf)).await?;
///
///     if n == 0 {
///         break;
///     }
/// }
///
/// // one or more reads might be necessary.
/// assert!(reads >= 1);
/// assert_eq!(&buf[..], &[0, 1, 2, 3]);
/// # Ok(())
/// # }
/// ```
pub fn poll_read_buf<R, B>(
    read: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut B,
) -> Poll<io::Result<usize>>
where
    R: AsyncRead,
    B: BufMut,
{
    if !buf.has_remaining_mut() {
        return Poll::Ready(Ok(0));
    }

    let n = {
        let mut buf = ReadBuf::uninit(buf.bytes_mut());
        let before = buf.filled().as_ptr();

        ready!(read.poll_read(cx, &mut buf)?);

        // This prevents a malicious read implementation from swapping out the
        // buffer being read, which would allow `filled` to be advanced without
        // actually initializing the provided buffer.
        //
        // We avoid this by asserting that the `ReadBuf` instance wraps the same
        // memory address both before and after the poll. Which will panic in
        // case its swapped.
        //
        // See https://github.com/tokio-rs/tokio/issues/2827 for more info.
        assert! {
            std::ptr::eq(before, buf.filled().as_ptr()),
            "Read buffer must not be changed during a read poll. \
            See https://github.com/tokio-rs/tokio/issues/2827 for more info."
        };

        buf.filled().len()
    };

    // Safety: This is guaranteed to be the number of initialized (and read)
    // bytes due to the invariants provided by `ReadBuf::filled`.
    unsafe {
        buf.advance_mut(n);
    }

    Poll::Ready(Ok(n))
}
