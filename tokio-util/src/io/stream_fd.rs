use std::io;
use std::io::{Error, Read, Write};
use std::os::unix::io::AsRawFd;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[macro_use]
mod ready {
    macro_rules! ready {
        ($e:expr $(,)?) => {
            match $e {
                std::task::Poll::Ready(t) => t,
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        };
    }
}

/// Provides async reading and writing semantics to a pollable file descriptor that is a byte
/// stream.
///
/// [`AsyncFd`] provides a way to poll file descriptors for IO readiness, but leaves reading and
/// writing to the user. This is a higher-level utility which handles this for users.
///
/// # Warning
/// The underlying IO source this is constructed from must not be capable of nonblocking reads and
/// writes, and must be pollable.
///  
/// The underlying IO source must also be a continuous stream of bytes in either direction. It must
/// be guaranteed that a partial read or write signals a loss of readiness.
///
/// The underlying IO source must also be self-flushing. This will assume that flushing is a no-op.
///
///
/// [`AsyncFd`]: struct@tokio::io::unix::AsyncFd
#[derive(Debug)]
pub struct StreamFd<T>
where
    T: AsRawFd,
{
    inner: AsyncFd<T>,
}

impl<T> StreamFd<T>
where
    T: AsRawFd,
{
    /// Construct a new StreamFd from an IO source.
    ///
    /// # Panics
    /// Panics if called from outside a tokio runtime context.
    ///
    /// [`RawFd`]: struct@std::os::fd::RawFd
    pub fn new(fd: T) -> io::Result<Self> {
        let inner = AsyncFd::new(fd)?;

        Ok(Self { inner })
    }
}

// note: taken from PollEvented
impl<T> AsyncRead for StreamFd<T>
where
    T: AsRawFd + Read + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        loop {
            let mut guard = ready!(this.inner.poll_read_ready_mut(cx))?;

            // safety: we will not be reading these bytes
            let b = unsafe {
                &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8])
            };
            let len = b.len();

            match guard.get_inner_mut().read(b) {
                Ok(n) => {
                    if n > 0 && n < len {
                        guard.clear_ready();
                    }

                    // Safety: We trust `File::read` to have filled up `n` bytes in the
                    // buffer.
                    unsafe { buf.assume_init(n) };
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    guard.clear_ready();
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }
}

impl<T> AsyncWrite for StreamFd<T>
where
    T: AsRawFd + Write + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        let this = self.get_mut();

        loop {
            let mut guard = ready!(this.inner.poll_write_ready_mut(cx))?;

            match guard.get_inner_mut().write(buf) {
                Ok(n) => {
                    // if we write only part of our buffer, this is sufficient on unix to show
                    // that the socket buffer is full
                    if n > 0 && n < buf.len() {
                        guard.clear_ready();
                    }

                    return Poll::Ready(Ok(n));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    guard.clear_ready();
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        unimplemented!("Shutdown is not implemented for this type")
    }
}
