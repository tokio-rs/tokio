use crate::{AsyncBufRead, AsyncRead};
use futures_core::ready;
use pin_utils::{unsafe_pinned, unsafe_unpinned};
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream for the [`chain`](super::AsyncReadExt::chain) method.
#[must_use = "streams do nothing unless polled"]
pub struct Chain<T, U> {
    first: T,
    second: U,
    done_first: bool,
}

impl<T, U> Unpin for Chain<T, U>
where
    T: Unpin,
    U: Unpin,
{
}

pub(super) fn chain<T, U>(first: T, second: U) -> Chain<T, U>
where
    T: AsyncRead,
    U: AsyncRead,
{
    Chain {
        first,
        second,
        done_first: false,
    }
}

impl<T, U> Chain<T, U>
where
    T: AsyncRead,
    U: AsyncRead,
{
    unsafe_pinned!(first: T);
    unsafe_pinned!(second: U);
    unsafe_unpinned!(done_first: bool);

    /// Gets references to the underlying readers in this `Chain`.
    pub fn get_ref(&self) -> (&T, &U) {
        (&self.first, &self.second)
    }

    /// Gets mutable references to the underlying readers in this `Chain`.
    ///
    /// Care should be taken to avoid modifying the internal I/O state of the
    /// underlying readers as doing so may corrupt the internal state of this
    /// `Chain`.
    pub fn get_mut(&mut self) -> (&mut T, &mut U) {
        (&mut self.first, &mut self.second)
    }

    /// Gets pinned mutable references to the underlying readers in this `Chain`.
    ///
    /// Care should be taken to avoid modifying the internal I/O state of the
    /// underlying readers as doing so may corrupt the internal state of this
    /// `Chain`.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> (Pin<&mut T>, Pin<&mut U>) {
        unsafe {
            let Self { first, second, .. } = self.get_unchecked_mut();
            (Pin::new_unchecked(first), Pin::new_unchecked(second))
        }
    }

    /// Consumes the `Chain`, returning the wrapped readers.
    pub fn into_inner(self) -> (T, U) {
        (self.first, self.second)
    }
}

impl<T, U> fmt::Debug for Chain<T, U>
where
    T: fmt::Debug,
    U: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chain")
            .field("t", &self.first)
            .field("u", &self.second)
            .finish()
    }
}

impl<T, U> AsyncRead for Chain<T, U>
where
    T: AsyncRead,
    U: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if !self.done_first {
            match ready!(self.as_mut().first().poll_read(cx, buf)?) {
                0 if !buf.is_empty() => *self.as_mut().done_first() = true,
                n => return Poll::Ready(Ok(n)),
            }
        }
        self.second().poll_read(cx, buf)
    }
}

impl<T, U> AsyncBufRead for Chain<T, U>
where
    T: AsyncBufRead,
    U: AsyncBufRead,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let Self {
            first,
            second,
            done_first,
        } = unsafe { self.get_unchecked_mut() };
        let first = unsafe { Pin::new_unchecked(first) };
        let second = unsafe { Pin::new_unchecked(second) };

        if !*done_first {
            match ready!(first.poll_fill_buf(cx)?) {
                buf if buf.is_empty() => {
                    *done_first = true;
                }
                buf => return Poll::Ready(Ok(buf)),
            }
        }
        second.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        if !self.done_first {
            self.first().consume(amt)
        } else {
            self.second().consume(amt)
        }
    }
}
