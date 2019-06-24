use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_io::{AsyncRead, AsyncWrite};

macro_rules! ready {
    ($e:expr) => {
        match $e {
            ::std::task::Poll::Ready(t) => t,
            ::std::task::Poll::Pending => return ::std::task::Poll::Pending,
        }
    };
}

/// A future which will copy all data from a reader into a writer.
///
/// Created by the [`copy`] function, this future will resolve to the number of
/// bytes copied or an error if one happens.
///
/// [`copy`]: fn.copy.html
#[derive(Debug)]
pub struct Copy<'a, R, W> {
    reader: &'a mut R,
    read_done: bool,
    writer: &'a mut W,
    pos: usize,
    cap: usize,
    amt: u64,
    buf: Box<[u8]>,
}

/// Creates a future which represents copying all the bytes from one object to
/// another.
///
/// The returned future will copy all the bytes read from `reader` into the
/// `writer` specified. This future will only complete once the `reader` has hit
/// EOF and all bytes have been written to and flushed from the `writer`
/// provided.
///
/// On success the number of bytes is returned and the `reader` and `writer` are
/// consumed. On error the error is returned and the I/O objects are consumed as
/// well.
pub fn copy<'a, R, W>(reader: &'a mut R, writer: &'a mut W) -> Copy<'a, R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
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

impl<'a, R, W> Future for Copy<'a, R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
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

            // If we've written al the data and we've seen EOF, flush out the
            // data and finish the transfer.
            // done with the entire transfer.
            if self.pos == self.cap && self.read_done {
                let me = &mut *self;
                ready!(Pin::new(&mut *me.writer).poll_flush(cx))?;
                return Poll::Ready(Ok(self.amt));
            }
        }
    }
}
