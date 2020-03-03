use crate::io::util::DEFAULT_BUF_SIZE;
use crate::io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite};

use pin_project_lite::pin_project;
use std::io::{self, Read, SeekFrom};
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, fmt};

pin_project! {
    /// The `BufReader` struct adds buffering to any reader.
    ///
    /// It can be excessively inefficient to work directly with a [`AsyncRead`]
    /// instance. A `BufReader` performs large, infrequent reads on the underlying
    /// [`AsyncRead`] and maintains an in-memory buffer of the results.
    ///
    /// `BufReader` can improve the speed of programs that make *small* and
    /// *repeated* read calls to the same file or network socket. It does not
    /// help when reading very large amounts at once, or reading just one or a few
    /// times. It also provides no advantage when reading from a source that is
    /// already in memory, like a `Vec<u8>`.
    ///
    /// When the `BufReader` is dropped, the contents of its buffer will be
    /// discarded. Creating multiple instances of a `BufReader` on the same
    /// stream can cause data loss.
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct BufReader<R> {
        #[pin]
        pub(super) inner: R,
        pub(super) buf: Box<[u8]>,
        pub(super) pos: usize,
        pub(super) cap: usize,
    }
}

impl<R: AsyncRead> BufReader<R> {
    /// Creates a new `BufReader` with a default buffer capacity. The default is currently 8 KB,
    /// but may change in the future.
    pub fn new(inner: R) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufReader` with the specified buffer capacity.
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        unsafe {
            let mut buffer = Vec::with_capacity(capacity);
            buffer.set_len(capacity);

            {
                // Convert to MaybeUninit
                let b = &mut *(&mut buffer[..] as *mut [u8] as *mut [MaybeUninit<u8>]);
                inner.prepare_uninitialized_buffer(b);
            }
            Self {
                inner,
                buf: buffer.into_boxed_slice(),
                pos: 0,
                cap: 0,
            }
        }
    }

    /// Gets a reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    /// Consumes this `BufWriter`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Returns a reference to the internally buffered data.
    ///
    /// Unlike `fill_buf`, this will not attempt to fill the buffer if it is empty.
    pub fn buffer(&self) -> &[u8] {
        &self.buf[self.pos..self.cap]
    }

    /// Invalidates all data in the internal buffer.
    #[inline]
    fn discard_buffer(self: Pin<&mut Self>) {
        let me = self.project();
        *me.pos = 0;
        *me.cap = 0;
    }
}

impl<R: AsyncRead> AsyncRead for BufReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.pos == self.cap && buf.len() >= self.buf.len() {
            let res = ready!(self.as_mut().get_pin_mut().poll_read(cx, buf));
            self.discard_buffer();
            return Poll::Ready(res);
        }
        let mut rem = ready!(self.as_mut().poll_fill_buf(cx))?;
        let nread = rem.read(buf)?;
        self.consume(nread);
        Poll::Ready(Ok(nread))
    }

    // we can't skip unconditionally because of the large buffer case in read.
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [MaybeUninit<u8>]) -> bool {
        self.inner.prepare_uninitialized_buffer(buf)
    }
}

impl<R: AsyncRead> AsyncBufRead for BufReader<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let me = self.project();

        // If we've reached the end of our internal buffer then we need to fetch
        // some more data from the underlying reader.
        // Branch using `>=` instead of the more correct `==`
        // to tell the compiler that the pos..cap slice is always valid.
        if *me.pos >= *me.cap {
            debug_assert!(*me.pos == *me.cap);
            *me.cap = ready!(me.inner.poll_read(cx, me.buf))?;
            *me.pos = 0;
        }
        Poll::Ready(Ok(&me.buf[*me.pos..*me.cap]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        *me.pos = cmp::min(*me.pos + amt, *me.cap);
    }
}

impl<R: AsyncRead + AsyncSeek> AsyncSeek for BufReader<R> {
    /// Seek to an offset, in bytes, in the underlying reader.
    ///
    /// The position used for seeking with `SeekFrom::Current(_)` is the
    /// position the underlying reader would be at if the `BufReader` had no
    /// internal buffer.
    ///
    /// Seeking always discards the internal buffer, even if the seek position
    /// would otherwise fall within it. This guarantees that calling
    /// `.into_inner()` immediately after a seek yields the underlying reader
    /// at the same position.
    ///
    /// See [`AsyncSeek`] for more details.
    ///
    /// Note: In the edge case where you're seeking with `SeekFrom::Current(n)`
    /// where `n` minus the internal buffer length overflows an `i64`, two
    /// seeks will be performed instead of one. If the second seek returns
    /// `Err`, the underlying reader will be left at the same position it would
    /// have if you called `seek` with `SeekFrom::Current(0)`.
    fn start_seek(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        position: SeekFrom,
    ) -> Poll<io::Result<()>> {
        if let SeekFrom::Current(n) = position {
            let remainder = (self.cap - self.pos) as i64;
            // it should be safe to assume that remainder fits within an i64 as the alternative
            // means we managed to allocate 8 exbibytes and that's absurd.
            // But it's not out of the realm of possibility for some weird underlying reader to
            // support seeking by i64::min_value() so we need to handle underflow when subtracting
            // remainder.
            if let Some(offset) = n.checked_sub(remainder) {
                ready!(self
                    .as_mut()
                    .get_pin_mut()
                    .start_seek(cx, SeekFrom::Current(offset)))?;
            } else {
                // seek backwards by our remainder, and then by the offset
                ready!(self
                    .as_mut()
                    .get_pin_mut()
                    .start_seek(cx, SeekFrom::Current(-remainder)))?;
                self.as_mut().discard_buffer();
                ready!(self
                    .as_mut()
                    .get_pin_mut()
                    .start_seek(cx, SeekFrom::Current(n)))?;
            }
        } else {
            ready!(self.as_mut().get_pin_mut().start_seek(cx, position))?;
        }

        self.discard_buffer();
        Poll::Ready(Ok(()))
    }

    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.get_pin_mut().poll_complete(cx)
    }
}

impl<R: AsyncRead + AsyncWrite> AsyncWrite for BufReader<R> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_pin_mut().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_pin_mut().poll_shutdown(cx)
    }
}

impl<R: fmt::Debug> fmt::Debug for BufReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufReader")
            .field("reader", &self.inner)
            .field(
                "buffer",
                &format_args!("{}/{}", self.cap - self.pos, self.buf.len()),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<BufReader<()>>();
    }
}
