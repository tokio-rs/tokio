use crate::io::{AsyncBufRead, AsyncRead};

use bytes::BufMut;
use futures_core::ready;
use pin_project::{pin_project, project};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io, mem::MaybeUninit};

/// Stream for the [`take`](super::AsyncReadExt::take) method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless you `.await` or poll them"]
pub struct Take<R> {
    #[pin]
    inner: R,
    // Add '_' to avoid conflicts with `limit` method.
    limit_: u64,
}

pub(super) fn take<R: AsyncRead>(inner: R, limit: u64) -> Take<R> {
    Take {
        inner,
        limit_: limit,
    }
}

impl<R: AsyncRead> Take<R> {
    /// Returns the remaining number of bytes that can be
    /// read before this instance will return EOF.
    ///
    /// # Note
    ///
    /// This instance may reach `EOF` after reading fewer bytes than indicated by
    /// this method if the underlying [`AsyncRead`] instance reaches EOF.
    pub fn limit(&self) -> u64 {
        self.limit_
    }

    /// Sets the number of bytes that can be read before this instance will
    /// return EOF. This is the same as constructing a new `Take` instance, so
    /// the amount of bytes read and the previous limit value don't matter when
    /// calling this method.
    pub fn set_limit(&mut self, limit: u64) {
        self.limit_ = limit
    }

    /// Gets a reference to the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// Care should be taken to avoid modifying the internal I/O state of the
    /// underlying reader as doing so may corrupt the internal limit of this
    /// `Take`.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying reader.
    ///
    /// Care should be taken to avoid modifying the internal I/O state of the
    /// underlying reader as doing so may corrupt the internal limit of this
    /// `Take`.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    /// Consumes the `Take`, returning the wrapped reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: AsyncRead> AsyncRead for Take<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut dyn BufMut,
    ) -> Poll<Result<usize, io::Error>> {
        if self.limit_ == 0 {
            return Poll::Ready(Ok(0));
        }

        let me = self.project();
        let n = ready!(me.inner.poll_read(
            cx,
            &mut Limit {
                inner: buf,
                limit: *me.limit_ as usize,
            }
        ))?;
        *me.limit_ -= n as u64;
        Poll::Ready(Ok(n))
    }
}

impl<R: AsyncBufRead> AsyncBufRead for Take<R> {
    #[project]
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        #[project]
        let Take { inner, limit_ } = self.project();

        // Don't call into inner reader at all at EOF because it may still block
        if *limit_ == 0 {
            return Poll::Ready(Ok(&[]));
        }

        let buf = ready!(inner.poll_fill_buf(cx)?);
        let cap = cmp::min(buf.len() as u64, *limit_) as usize;
        Poll::Ready(Ok(&buf[..cap]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        // Don't let callers reset the limit by passing an overlarge value
        let amt = cmp::min(amt as u64, *me.limit_) as usize;
        *me.limit_ -= amt as u64;
        me.inner.consume(amt);
    }
}

// TODO: replace with type from BufMutExt when it exists
struct Limit<T> {
    inner: T,
    limit: usize,
}

impl<T: BufMut> BufMut for Limit<T> {
    fn remaining_mut(&self) -> usize {
        cmp::min(self.inner.remaining_mut(), self.limit)
    }

    fn bytes_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        let bytes = self.inner.bytes_mut();
        let end = cmp::min(bytes.len(), self.limit);
        &mut bytes[..end]
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        assert!(cnt <= self.limit);
        self.inner.advance_mut(cnt);
        self.limit -= cnt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<Take<()>>();
    }
}
