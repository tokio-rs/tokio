//! Helpers for IO related tasks.
//!
//! These types are often used in combination with hyper or reqwest, as they
//! allow converting between a hyper [`Body`] and [`AsyncRead`].
//!
//! [`Body`]: https://docs.rs/hyper/0.13/hyper/struct.Body.html
//! [`AsyncRead`]: tokio::io::AsyncRead

mod reader_stream;
mod stream_reader;

pub use self::reader_stream::ReaderStream;
pub use self::stream_reader::StreamReader;

use tokio::io::{AsyncRead, ReadBuf};

use bytes::BufMut;
use futures_core::ready;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) fn poll_read_buf<T: AsyncRead>(
    cx: &mut Context<'_>,
    io: Pin<&mut T>,
    buf: &mut impl BufMut,
) -> Poll<io::Result<usize>> {
    if !buf.has_remaining_mut() {
        return Poll::Ready(Ok(0));
    }

    let mut b = ReadBuf::uninit(buf.bytes_mut());

    ready!(io.poll_read(cx, &mut b))?;
    let n = b.filled().len();

    // Safety: we can assume `n` bytes were read, since they are in`filled`.
    unsafe {
        buf.advance_mut(n);
    }
    Poll::Ready(Ok(n))
}
