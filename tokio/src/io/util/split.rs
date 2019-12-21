use crate::io::util::read_until::read_until_internal;
use crate::io::AsyncBufRead;

use pin_project_lite::pin_project;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Stream for the [`split`](crate::io::AsyncBufReadExt::split) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct Split<R> {
        #[pin]
        reader: R,
        buf: Vec<u8>,
        delim: u8,
        read: usize,
    }
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

        poll_fn(|cx| Pin::new(&mut *self).poll_next_segment(cx)).await
    }
}

impl<R> Split<R>
where
    R: AsyncBufRead,
{
    #[doc(hidden)]
    pub fn poll_next_segment(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<Option<Vec<u8>>>> {
        let me = self.project();

        let n = ready!(read_until_internal(
            me.reader, cx, *me.delim, me.buf, me.read,
        ))?;

        if n == 0 && me.buf.is_empty() {
            return Poll::Ready(Ok(None));
        }

        if me.buf.last() == Some(me.delim) {
            me.buf.pop();
        }

        Poll::Ready(Ok(Some(mem::replace(me.buf, Vec::new()))))
    }
}

#[cfg(feature = "stream")]
impl<R: AsyncBufRead> crate::stream::Stream for Split<R> {
    type Item = io::Result<Vec<u8>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(match ready!(self.poll_next_segment(cx)) {
            Ok(Some(segment)) => Some(Ok(segment)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        })
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
