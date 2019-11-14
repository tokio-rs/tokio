use crate::io::util::read_line::read_line_internal;
use crate::io::AsyncBufRead;

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

impl<R> Lines<R>
where
    R: AsyncBufRead + Unpin,
{
    /// Returns the next line in the stream.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::io::AsyncBufRead;
    /// use tokio::io::AsyncBufReadExt;
    ///
    /// # async fn dox(my_buf_read: impl AsyncBufRead + Unpin) -> std::io::Result<()> {
    /// let mut lines = my_buf_read.lines();
    ///
    /// while let Some(line) = lines.next_line().await? {
    ///     println!("length = {}", line.len())
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn next_line(&mut self) -> io::Result<Option<String>> {
        use crate::future::poll_fn;

        poll_fn(|cx| self.poll_next_line(cx)).await
    }

    #[doc(hidden)]
    pub fn poll_next_line(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Option<String>>> {
        let n = ready!(read_line_internal(
            Pin::new(&mut self.reader),
            cx,
            &mut self.buf,
            &mut self.bytes,
            &mut self.read
        ))?;

        if n == 0 && self.buf.is_empty() {
            return Poll::Ready(Ok(None));
        }

        if self.buf.ends_with('\n') {
            self.buf.pop();

            if self.buf.ends_with('\r') {
                self.buf.pop();
            }
        }

        Poll::Ready(Ok(Some(mem::replace(&mut self.buf, String::new()))))
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
