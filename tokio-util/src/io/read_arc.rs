use std::io;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Read data from an `AsyncRead` into an `Arc`.
///
/// This uses `Arc::new_uninit_slice` and reads into the resulting uninitialized `Arc`.
///
/// # Example
///
/// ```
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// use tokio_util::io::read_exact_arc;
///
/// let read = tokio::io::repeat(42);
///
/// let arc = read_exact_arc(read, 4).await?;
///
/// assert_eq!(&arc[..], &[42; 4]);
/// # Ok(())
/// # }
/// ```
pub async fn read_exact_arc<R>(mut read: R, len: usize) -> io::Result<Arc<[u8]>>
where
    R: AsyncRead + Unpin,
{
    let mut arc = Arc::new_uninit_slice(len);
    let mut buf = unsafe { Arc::get_mut(&mut arc).unwrap_unchecked() };
    let mut bytes_remaining = len;
    while bytes_remaining != 0 {
        let bytes_read = read.read_buf(&mut buf).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "early eof"));
        }
        bytes_remaining -= bytes_read;
    }
    Ok(unsafe { arc.assume_init() })
}
