use crate::io::util::read_line::read_line_internal;
use crate::io::AsyncBufRead;

use pin_project_lite::pin_project;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Stream for the [`lines`](crate::io::AsyncBufReadExt::lines) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct Lines<R> {
        #[pin]
        reader: R,
        buf: String,
        bytes: Vec<u8>,
        read: usize,
    }
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

        poll_fn(|cx| Pin::new(&mut *self).poll_next_line(cx)).await
    }

    /// Obtain a mutable reference to the underlying reader
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Obtain a reference to the underlying reader
    pub fn get_ref(&mut self) -> &R {
        &self.reader
    }

    /// Unwraps this `Lines<R>`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    /// Therefore, a following read from the underlying reader may lead to data loss.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R> Lines<R>
where
    R: AsyncBufRead,
{
    fn poll_next_line(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<Option<String>>> {
        let me = self.project();

        let n = ready!(read_line_internal(me.reader, cx, me.buf, me.bytes, me.read))?;
        debug_assert_eq!(*me.read, 0);

        if n == 0 && me.buf.is_empty() {
            return Poll::Ready(Ok(None));
        }

        if me.buf.ends_with('\n') {
            me.buf.pop();

            if me.buf.ends_with('\r') {
                me.buf.pop();
            }
        }

        Poll::Ready(Ok(Some(mem::replace(me.buf, String::new()))))
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
