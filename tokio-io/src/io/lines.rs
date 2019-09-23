use super::read_line::read_line_internal;
use crate::AsyncBufRead;

use futures_core::{ready, Stream};
use pin_project::{pin_project, project};
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream for the [`lines`](crate::io::AsyncBufReadExt::lines) method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Lines<R> {
    #[pin]
    reader: R,
    buf: String,
    bytes: Vec<u8>,
    read: usize,
}

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

    #[project]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        #[project]
        let Lines {
            reader,
            buf,
            bytes,
            read,
        } = self.project();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<Lines<()>>();
    }
}
