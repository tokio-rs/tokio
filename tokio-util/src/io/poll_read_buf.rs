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
        ready!(read.poll_read(cx, &mut buf)?);
        buf.filled().len()
    };

    // Safety: This is guaranteed to be the number of initialized (and read)
    // bytes due to the invariants provided by `ReadBuf::filled`.
    unsafe {
        buf.advance_mut(n);
    }

    Poll::Ready(Ok(n))
}
