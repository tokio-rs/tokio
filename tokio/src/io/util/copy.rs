use crate::io::{AsyncRead, AsyncWrite, ReadBuf};

use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
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
        // Make this future `!Unpin` for compatibility with async functions.
        #[pin]
        _pin: PhantomPinned,
    }
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
            buf: vec![0; 2048].into_boxed_slice(),
            _pin: PhantomPinned,
        }
    }
}

impl<R, W> Future for Copy<'_, R, W>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<u64>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let mut me = self.project();
        loop {
            // If our buffer is empty, then we need to read some data to
            // continue.
            if *me.pos == *me.cap && !*me.read_done {
                let mut buf = ReadBuf::new(&mut me.buf);
                ready!(Pin::new(&mut *me.reader).poll_read(cx, &mut buf))?;
                let n = buf.filled().len();
                if n == 0 {
                    *me.read_done = true;
                } else {
                    *me.pos = 0;
                    *me.cap = n;
                }
            }

            // If our buffer has some data, let's write it out!
            while *me.pos < *me.cap {
                let i =
                    ready!(Pin::new(&mut *me.writer).poll_write(cx, &me.buf[*me.pos..*me.cap]))?;
                if i == 0 {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "write zero byte into writer",
                    )));
                } else {
                    *me.pos += i;
                    *me.amt += i as u64;
                }
            }

            // If we've written all the data and we've seen EOF, flush out the
            // data and finish the transfer.
            if *me.pos == *me.cap && *me.read_done {
                ready!(Pin::new(&mut *me.writer).poll_flush(cx))?;
                return Poll::Ready(Ok(*me.amt));
            }
        }
    }
}
