use bytes::BufMut;
use std::io;
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Read bytes asynchronously.
///
/// This trait inherits from `std::io::Read` and indicates that an I/O object is
/// **non-blocking**. All non-blocking I/O objects must return an error when
/// bytes are unavailable instead of blocking the current thread.
///
/// Specifically, this means that the `poll_read` function will return one of
/// the following:
///
/// * `Poll::Ready(Ok(n))` means that `n` bytes of data was immediately read
///   and placed into the output buffer, where `n` == 0 implies that EOF has
///   been reached.
///
/// * `Poll::Pending` means that no data was read into the buffer
///   provided. The I/O object is not currently readable but may become readable
///   in the future. Most importantly, **the current future's task is scheduled
///   to get unparked when the object is readable**. This means that like
///   `Future::poll` you'll receive a notification when the I/O object is
///   readable again.
///
/// * `Poll::Ready(Err(e))` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the `read` method only works in the
/// context of a future's task. The object may panic if used outside of a task.
pub trait AsyncRead {
    /// Attempt to read from the `AsyncRead` into `buf`.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_read))`.
    ///
    /// If no data is available for reading, the method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker()`) to receive a notification when the object becomes
    /// readable or is closed.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut dyn BufMut,
    ) -> Poll<io::Result<usize>>;
}

macro_rules! deref_async_read {
    () => {
        fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut dyn BufMut)
            -> Poll<io::Result<usize>>
        {
            Pin::new(&mut **self).poll_read(cx, buf)
        }
    }
}

impl<T: ?Sized + AsyncRead + Unpin> AsyncRead for Box<T> {
    deref_async_read!();
}

impl<T: ?Sized + AsyncRead + Unpin> AsyncRead for &mut T {
    deref_async_read!();
}

impl<P> AsyncRead for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut dyn BufMut,
    ) -> Poll<io::Result<usize>> {
        self.get_mut().as_mut().poll_read(cx, buf)
    }
}

impl AsyncRead for &[u8] {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut dyn BufMut,
    ) -> Poll<io::Result<usize>> {
        if self.len() > buf.remaining_mut() {
            let n = buf.remaining_mut();
            let (a, b) = self.split_at(n);
            buf.put_slice(a);
            *self.get_mut() = b;
            Poll::Ready(Ok(n))
        } else {
            let n = self.len();
            buf.put_slice(&*self);
            Poll::Ready(Ok(n))
        }
    }
}

impl<T: AsRef<[u8]> + Unpin> AsyncRead for io::Cursor<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        mut buf: &mut dyn BufMut,
    ) -> Poll<io::Result<usize>> {
        use bytes::{buf::BufExt, Buf};

        if self.as_mut().get_mut().remaining() > buf.remaining_mut() {
            let n = buf.remaining_mut();
            BufMut::put(&mut buf, self.as_mut().get_mut().take(n));
            Poll::Ready(Ok(n))
        } else {
            let n = self.as_mut().get_mut().remaining();
            BufMut::put(&mut buf, self.get_mut());
            Poll::Ready(Ok(n))
        }
    }
}
