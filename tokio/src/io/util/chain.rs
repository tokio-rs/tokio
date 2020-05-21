use crate::io::{AsyncBufRead, AsyncRead};

use pin_project_lite::pin_project;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Stream for the [`chain`](super::AsyncReadExt::chain) method.
    #[must_use = "streams do nothing unless polled"]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct Chain<T, U> {
        #[pin]
        first: T,
        #[pin]
        second: U,
        done_first: bool,
    }
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
        let me = self.project();
        (me.first, me.second)
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
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [std::mem::MaybeUninit<u8>]) -> bool {
        if self.first.prepare_uninitialized_buffer(buf) {
            return true;
        }
        if self.second.prepare_uninitialized_buffer(buf) {
            return true;
        }
        false
    }
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.project();

        if !*me.done_first {
            match ready!(me.first.poll_read(cx, buf)?) {
                0 if !buf.is_empty() => *me.done_first = true,
                n => return Poll::Ready(Ok(n)),
            }
        }
        me.second.poll_read(cx, buf)
    }
}

impl<T, U> AsyncBufRead for Chain<T, U>
where
    T: AsyncBufRead,
    U: AsyncBufRead,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let me = self.project();

        if !*me.done_first {
            match ready!(me.first.poll_fill_buf(cx)?) {
                buf if buf.is_empty() => {
                    *me.done_first = true;
                }
                buf => return Poll::Ready(Ok(buf)),
            }
        }
        me.second.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        if !*me.done_first {
            me.first.consume(amt)
        } else {
            me.second.consume(amt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<Chain<(), ()>>();
    }
}
