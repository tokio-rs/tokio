//! Split a single value implementing `AsyncRead + AsyncWrite` into separate
//! `AsyncRead` and `AsyncWrite` handles.
//!
//! To restore this read/write object from its `split::ReadHalf` and
//! `split::WriteHalf` use `unsplit`.

use crate::io::{AsyncRead, AsyncWrite};

use bytes::{Buf, BufMut};
use futures_core::ready;
use std::cell::UnsafeCell;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;
use std::task::{Context, Poll};

/// The readable half of a value returned from `split`.
pub struct ReadHalf<T> {
    inner: Arc<Inner<T>>,
}

/// The writable half of a value returned from `split`.
pub struct WriteHalf<T> {
    inner: Arc<Inner<T>>,
}

struct Inner<T> {
    locked: AtomicBool,
    stream: UnsafeCell<T>,
}

struct Guard<'a, T> {
    inner: &'a Inner<T>,
}

/// Split a single value implementing `AsyncRead + AsyncWrite` into separate
/// `AsyncRead` and `AsyncWrite` handles.
///
/// To restore this read/write object from its `split::ReadHalf` and
/// `split::WriteHalf` use `unsplit`.
pub fn split<T>(stream: T) -> (ReadHalf<T>, WriteHalf<T>)
where
    T: AsyncRead + AsyncWrite,
{
    let inner = Arc::new(Inner {
        locked: AtomicBool::new(false),
        stream: UnsafeCell::new(stream),
    });

    let rd = ReadHalf {
        inner: inner.clone(),
    };

    let wr = WriteHalf { inner };

    (rd, wr)
}

impl<T> ReadHalf<T> {
    /// Reunite with a previously split `WriteHalf`.
    ///
    /// # Panics
    ///
    /// If this `ReadHalf` and the given `WriteHalf` do not originate from the
    /// same `split` operation this method will panic.
    pub fn unsplit(self, wr: WriteHalf<T>) -> T {
        if Arc::ptr_eq(&self.inner, &wr.inner) {
            drop(wr);

            let inner = Arc::try_unwrap(self.inner)
                .ok()
                .expect("Arc::try_unwrap failed");

            inner.stream.into_inner()
        } else {
            panic!("Unrelated `split::Write` passed to `split::Read::unsplit`.")
        }
    }
}

impl<T: AsyncRead> AsyncRead for ReadHalf<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_read(cx, buf)
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_read_buf(cx, buf)
    }
}

impl<T: AsyncWrite> AsyncWrite for WriteHalf<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_shutdown(cx)
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = ready!(self.inner.poll_lock(cx));
        inner.stream_pin().poll_write_buf(cx, buf)
    }
}

impl<T> Inner<T> {
    fn poll_lock(&self, cx: &mut Context<'_>) -> Poll<Guard<'_, T>> {
        if !self.locked.compare_and_swap(false, true, Acquire) {
            Poll::Ready(Guard { inner: self })
        } else {
            // Spin... but investigate a better strategy

            ::std::thread::yield_now();
            cx.waker().wake_by_ref();

            Poll::Pending
        }
    }
}

impl<T> Guard<'_, T> {
    fn stream_pin(&mut self) -> Pin<&mut T> {
        // safety: the stream is pinned in `Arc` and the `Guard` ensures mutual
        // exclusion.
        unsafe { Pin::new_unchecked(&mut *self.inner.stream.get()) }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.inner.locked.store(false, Release);
    }
}

unsafe impl<T: Send> Send for ReadHalf<T> {}
unsafe impl<T: Send> Send for WriteHalf<T> {}
unsafe impl<T: Sync> Sync for ReadHalf<T> {}
unsafe impl<T: Sync> Sync for WriteHalf<T> {}

impl<T: fmt::Debug> fmt::Debug for ReadHalf<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("split::ReadHalf").finish()
    }
}

impl<T: fmt::Debug> fmt::Debug for WriteHalf<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("split::WriteHalf").finish()
    }
}
