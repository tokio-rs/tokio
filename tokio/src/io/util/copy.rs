use crate::io::{AsyncRead, AsyncWrite, ReadBuf};

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug)]
pub(super) struct CopyBuffer {
    read_done: bool,
    pos: usize,
    cap: usize,
    amt: u64,
    buf: Box<[u8]>,
}

impl CopyBuffer {
    pub(super) fn new() -> Self {
        CopyBuffer::with_size(2048)
    }

    pub(super) fn with_size(size: usize) -> Self {
        Self {
            read_done: false,
            pos: 0,
            cap: 0,
            amt: 0,
            buf: vec![0; size].into_boxed_slice(),
        }
    }

    pub(super) fn poll_copy<R, W>(
        &mut self,
        cx: &mut Context<'_>,
        mut reader: Pin<&mut R>,
        mut writer: Pin<&mut W>,
    ) -> Poll<io::Result<u64>>
    where
        R: AsyncRead + ?Sized,
        W: AsyncWrite + ?Sized,
    {
        loop {
            // If our buffer is empty, then we need to read some data to
            // continue.
            if self.pos == self.cap && !self.read_done {
                let me = &mut *self;
                let mut buf = ReadBuf::new(&mut me.buf);
                ready!(reader.as_mut().poll_read(cx, &mut buf))?;
                let n = buf.filled().len();
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
                let i = ready!(writer.as_mut().poll_write(cx, &me.buf[me.pos..me.cap]))?;
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
                ready!(writer.as_mut().poll_flush(cx))?;
                return Poll::Ready(Ok(self.amt));
            }
        }
    }
}

/// A future that asynchronously copies the entire contents of a reader into a
/// writer.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct Copy<'a, R: ?Sized, W: ?Sized> {
    reader: &'a mut R,
    writer: &'a mut W,
    buf: CopyBuffer,
}

cfg_io_util! {
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
    /// The returned future will return an error immediately if any call to
    /// `poll_read` or `poll_write` returns an error.
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
    pub async fn copy<'a, R, W>(reader: &'a mut R, writer: &'a mut W) -> io::Result<u64>
    where
        R: AsyncRead + Unpin + ?Sized,
        W: AsyncWrite + Unpin + ?Sized,
    {
        Copy {
            reader,
            writer,
            buf: CopyBuffer::new()
        }.await
    }
}

impl<R, W> Future for Copy<'_, R, W>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let me = &mut *self;

        me.buf
            .poll_copy(cx, Pin::new(&mut *me.reader), Pin::new(&mut *me.writer))
    }
}
