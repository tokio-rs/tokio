use bytes::BufMut;
use std::io;
use std::mem::MaybeUninit;
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Reads bytes from a source.
///
/// This trait is analogous to the [`std::io::Read`] trait, but integrates with
/// the asynchronous task system. In particular, the [`poll_read`] method,
/// unlike [`Read::read`], will automatically queue the current task for wakeup
/// and return if data is not yet available, rather than blocking the calling
/// thread.
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
///
/// Utilities for working with `AsyncRead` values are provided by
/// [`AsyncReadExt`].
///
/// [`poll_read`]: AsyncRead::poll_read
/// [`std::io::Read`]: std::io::Read
/// [`Read::read`]: std::io::Read::read
/// [`AsyncReadExt`]: crate::io::AsyncReadExt
pub trait AsyncRead {
    /// Prepares an uninitialized buffer to be safe to pass to `read`. Returns
    /// `true` if the supplied buffer was zeroed out.
    ///
    /// While it would be highly unusual, implementations of [`io::Read`] are
    /// able to read data from the buffer passed as an argument. Because of
    /// this, the buffer passed to [`io::Read`] must be initialized memory. In
    /// situations where large numbers of buffers are used, constantly having to
    /// zero out buffers can be expensive.
    ///
    /// This function does any necessary work to prepare an uninitialized buffer
    /// to be safe to pass to `read`. If `read` guarantees to never attempt to
    /// read data out of the supplied buffer, then `prepare_uninitialized_buffer`
    /// doesn't need to do any work.
    ///
    /// If this function returns `true`, then the memory has been zeroed out.
    /// This allows implementations of `AsyncRead` which are composed of
    /// multiple subimplementations to efficiently implement
    /// `prepare_uninitialized_buffer`.
    ///
    /// This function isn't actually `unsafe` to call but `unsafe` to implement.
    /// The implementer must ensure that either the whole `buf` has been zeroed
    /// or `poll_read_buf()` overwrites the buffer without reading it and returns
    /// correct value.
    ///
    /// This function is called from [`poll_read_buf`].
    ///
    /// # Safety
    ///
    /// Implementations that return `false` must never read from data slices
    /// that they did not write to.
    ///
    /// [`io::Read`]: std::io::Read
    /// [`poll_read_buf`]: #method.poll_read_buf
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [MaybeUninit<u8>]) -> bool {
        for x in buf {
            *x.as_mut_ptr() = 0;
        }

        true
    }

    /// Attempts to read from the `AsyncRead` into `buf`.
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
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>>;

    /// Pulls some bytes from this source into the specified `BufMut`, returning
    /// how many bytes were read.
    ///
    /// The `buf` provided will have bytes read into it and the internal cursor
    /// will be advanced if any bytes were read. Note that this method typically
    /// will not reallocate the buffer provided.
    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>>
    where
        Self: Sized,
    {
        if !buf.has_remaining_mut() {
            return Poll::Ready(Ok(0));
        }

        unsafe {
            let n = {
                let b = buf.bytes_mut();

                self.prepare_uninitialized_buffer(b);

                // Convert to `&mut [u8]`
                let b = &mut *(b as *mut [MaybeUninit<u8>] as *mut [u8]);

                let n = ready!(self.poll_read(cx, b))?;
                assert!(n <= b.len(), "Bad AsyncRead implementation, more bytes were reported as read than the buffer can hold");
                n
            };

            buf.advance_mut(n);
            Poll::Ready(Ok(n))
        }
    }
}

macro_rules! deref_async_read {
    () => {
        unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [MaybeUninit<u8>]) -> bool {
            (**self).prepare_uninitialized_buffer(buf)
        }

        fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8])
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
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [MaybeUninit<u8>]) -> bool {
        (**self).prepare_uninitialized_buffer(buf)
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().as_mut().poll_read(cx, buf)
    }
}

impl AsyncRead for &[u8] {
    unsafe fn prepare_uninitialized_buffer(&self, _buf: &mut [MaybeUninit<u8>]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Read::read(self.get_mut(), buf))
    }
}

impl<T: AsRef<[u8]> + Unpin> AsyncRead for io::Cursor<T> {
    unsafe fn prepare_uninitialized_buffer(&self, _buf: &mut [MaybeUninit<u8>]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(io::Read::read(self.get_mut(), buf))
    }
}
