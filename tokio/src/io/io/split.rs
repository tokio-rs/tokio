use crate::io::io::read_until::read_until_internal;
use crate::io::AsyncBufRead;

use futures_core::{ready, Stream};
use pin_project::{pin_project, project};
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream for the [`split`](crate::io::AsyncBufReadExt::split) method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Split<R> {
    #[pin]
    reader: R,
    buf: Vec<u8>,
    delim: u8,
    read: usize,
}

pub(crate) fn split<R>(reader: R, delim: u8) -> Split<R>
where
    R: AsyncBufRead,
{
    Split {
        reader,
        buf: Vec::new(),
        delim,
        read: 0,
    }
}

impl<R: AsyncBufRead> Stream for Split<R> {
    type Item = io::Result<Vec<u8>>;

    #[project]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        #[project]
        let Split {
            reader,
            buf,
            delim,
            read,
        } = self.project();

        let n = ready!(read_until_internal(reader, cx, *delim, buf, read))?;
        if n == 0 && buf.is_empty() {
            return Poll::Ready(None);
        }
        if buf.last() == Some(&delim) {
            buf.pop();
        }
        Poll::Ready(Some(Ok(mem::replace(buf, Vec::new()))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<Split<()>>();
    }
}
