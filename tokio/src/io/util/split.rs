use crate::io::util::read_until::read_until_internal;
use crate::io::AsyncBufRead;

use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream for the [`split`](crate::io::AsyncBufReadExt::split) method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Split<R> {
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

impl<R> Split<R>
where
    R: AsyncBufRead + Unpin,
{
    /// Returns the next segment in the stream.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::io::AsyncBufRead;
    /// use tokio::io::AsyncBufReadExt;
    ///
    /// # async fn dox(my_buf_read: impl AsyncBufRead + Unpin) -> std::io::Result<()> {
    /// let mut segments = my_buf_read.split(b'f');
    ///
    /// while let Some(segment) = segments.next_segment().await? {
    ///     println!("length = {}", segment.len())
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn next_segment(&mut self) -> io::Result<Option<Vec<u8>>> {
        use crate::future::poll_fn;

        poll_fn(|cx| self.poll_next_segment(cx)).await
    }

    #[doc(hidden)]
    pub fn poll_next_segment(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Option<Vec<u8>>>> {
        let n = ready!(read_until_internal(
            Pin::new(&mut self.reader),
            cx,
            self.delim,
            &mut self.buf,
            &mut self.read
        ))?;

        if n == 0 && self.buf.is_empty() {
            return Poll::Ready(Ok(None));
        }

        if self.buf.last() == Some(&self.delim) {
            self.buf.pop();
        }

        Poll::Ready(Ok(Some(mem::replace(&mut self.buf, Vec::new()))))
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
