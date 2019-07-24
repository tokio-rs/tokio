use super::read_line::read_line_internal;

use tokio_io::AsyncBufRead;

use futures_core::{ready, Stream};
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream for the [`lines`](crate::io::AsyncBufReadExt::lines) method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Lines<R> {
    reader: R,
    buf: String,
    bytes: Vec<u8>,
    read: usize,
}

impl<R: Unpin> Unpin for Lines<R> {}

pub(crate) fn lines<R>(reader: R) -> Lines<R>
where
    R: AsyncBufRead,
{
    Lines {
        reader,
        buf: String::new(),
        bytes: Vec::new(),
        read: 0,
    }
}

impl<R: AsyncBufRead> Stream for Lines<R> {
    type Item = io::Result<String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Self {
            reader,
            buf,
            bytes,
            read,
        } = unsafe { self.get_unchecked_mut() };
        let reader = unsafe { Pin::new_unchecked(reader) };
        let n = ready!(read_line_internal(reader, cx, buf, bytes, read))?;
        if n == 0 && buf.is_empty() {
            return Poll::Ready(None);
        }
        if buf.ends_with('\n') {
            buf.pop();
            if buf.ends_with('\r') {
                buf.pop();
            }
        }
        Poll::Ready(Some(Ok(mem::replace(buf, String::new()))))
    }
}
