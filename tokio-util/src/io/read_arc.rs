use std::io;
use std::mem::MaybeUninit;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Read data from an `AsyncRead` into an `Arc`.
///
/// This uses `Arc::new_uninit_slice` and reads into the resulting uninitialized `Arc`.
///
/// Interrupted reads are retried. Other errors are returned immediately.
///
/// # Example
///
/// ```
/// # #[tokio::main(flavor = "current_thread")]
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
pub async fn read_exact_arc<R: AsyncRead>(read: R, len: usize) -> io::Result<Arc<[u8]>> {
    tokio::pin!(read);
    let arc = Arc::<[u8]>::new_uninit_slice(len);
    // TODO(MSRV future): Use `Arc::get_mut_unchecked` once it's stabilized.
    // SAFETY: We're the only owner of the `Arc`, and we keep the `Arc` valid throughout this loop
    // as we write through this reference.
    let mut buf = unsafe { &mut *(Arc::as_ptr(&arc) as *mut [MaybeUninit<u8>]) };
    while !buf.is_empty() {
        let n = match read.read_buf(&mut buf).await {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            res => res?,
        };
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "early eof"));
        }
    }
    // SAFETY: We've initialized all the bytes in the loop above.
    Ok(unsafe { arc.assume_init() })
}
