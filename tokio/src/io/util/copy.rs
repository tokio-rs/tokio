use crate::io::{AsyncRead, AsyncWrite};

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    /// A future that asynchronously copies the entire contents of a reader into a
    /// writer.
    ///
    /// This struct is generally created by calling [`copy`][copy]. Please
    /// see the documentation of `copy()` for more details.
    ///
    /// [copy]: copy()
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Copy<'a, R: ?Sized, W: ?Sized> {
        reader: &'a mut R,
        read_done: bool,
        writer: &'a mut W,
        pos: usize,
        cap: usize,
        amt: u64,
        buf: Box<[u8]>,
    }

    /// Asynchronously copies the entire contents of a reader into a writer.
    ///
    /// This function returns a future that will continuously read data from
    /// `reader` and then write it into `writer` in a streaming fashion until
    /// `reader` returns EOF.
    ///
    /// On success, the total number of bytes that were copied from `reader` to
    /// `writer` is returned.
    ///
    /// This is an asynchronous version of [`std::io::copy`][std].
    ///
    /// [std]: std::io::copy
    ///
    /// # Errors
    ///
    /// The returned future will finish with an error will return an error
    /// immediately if any call to `poll_read` or `poll_write` returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut reader: &[u8] = b"hello";
    /// let mut writer: Vec<u8> = vec![];
    ///
    /// io::copy(&mut reader, &mut writer).await?;
    ///
    /// assert_eq!(&b"hello"[..], &writer[..]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn copy<'a, R, W>(reader: &'a mut R, writer: &'a mut W) -> Copy<'a, R, W>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        Copy {
            reader,
            read_done: false,
            writer,
            amt: 0,
            pos: 0,
            cap: 0,
            buf: Box::new([0; 2048]),
        }
    }
}

impl<R, W> Future for Copy<'_, R, W>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        loop {
            // If our buffer is empty, then we need to read some data to
            // continue.
            if self.pos == self.cap && !self.read_done {
                let me = &mut *self;
                let n = ready!(Pin::new(&mut *me.reader).poll_read(cx, &mut me.buf))?;
                if n == 0 {
                    self.read_done = true;
                } else {
                    self.pos = 0;
                    self.cap = n;
                }
            }

            // If our buffer has some data, let's write it out!
            while self.pos < self.cap {
                let me = &mut *self;
                let i = ready!(Pin::new(&mut *me.writer).poll_write(cx, &me.buf[me.pos..me.cap]))?;
                if i == 0 {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "write zero byte into writer",
                    )));
                } else {
                    self.pos += i;
                    self.amt += i as u64;
                }
            }

            // If we've written all the data and we've seen EOF, flush out the
            // data and finish the transfer.
            if self.pos == self.cap && self.read_done {
                let me = &mut *self;
                ready!(Pin::new(&mut *me.writer).poll_flush(cx))?;
                return Poll::Ready(Ok(self.amt));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<Copy<'_, PhantomPinned, PhantomPinned>>();
    }
}
