use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{self, Poll};
use tokio_io::AsyncRead;

/// A future which can be used to read exactly enough bytes to fill a buffer.
#[derive(Debug)]
pub struct ReadExact<'a, T: ?Sized> {
    reader: &'a mut T,
    buf: &'a mut [u8],
}

// Pinning is never projected to fields
impl<'a, T: ?Sized> Unpin for ReadExact<'a, T> {}

impl<'a, T: AsyncRead + ?Sized> ReadExact<'a, T> {
    pub(super) fn new(reader: &'a mut T, buf: &'a mut [u8]) -> ReadExact<'a, T> {
        ReadExact { reader, buf }
    }
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")
}

impl<'a, T: AsyncRead + ?Sized> Future for ReadExact<'a, T> {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut task::Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;

        let this = &mut *self;

        while !this.buf.is_empty() {
            let n = try_ready!(convert_poll(this.reader.poll_read(this.buf)));

            {
                let (_, rest) = mem::replace(&mut this.buf, &mut []).split_at_mut(n);
                this.buf = rest;
            }
            if n == 0 {
                return Poll::Ready(Err(eof()));
            }
        }

        Poll::Ready(Ok(()))
    }
}
