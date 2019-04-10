use tokio_io::AsyncWrite;

use std::future::Future;
use std::task::{self, Poll};

use std::io;
use std::mem;
use std::pin::Pin;

/// A future used to write the entire contents of a buffer.
#[derive(Debug)]
pub struct WriteAll<'a, T: ?Sized + 'a> {
    writer: &'a mut T,
    buf: &'a [u8],
}

// Pinning is never projected to fields
impl<'a, T: ?Sized> Unpin for WriteAll<'a, T> {}

impl<'a, T: AsyncWrite + ?Sized> WriteAll<'a, T> {
    pub(super) fn new(writer: &'a mut T, buf: &'a [u8]) -> WriteAll<'a, T> {
        WriteAll { writer, buf }
    }
}

fn zero_write() -> io::Error {
    io::Error::new(io::ErrorKind::WriteZero, "zero-length write")
}

impl<'a, T: AsyncWrite + ?Sized> Future for WriteAll<'a, T> {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut task::Context) -> Poll<io::Result<()>> {
        use crate::compat::forward::convert_poll;

        let this = &mut *self;

        while !this.buf.is_empty() {
            let n = try_ready!(convert_poll(this.writer.poll_write(this.buf)));

            {
                let (_, rest) = mem::replace(&mut this.buf, &[]).split_at(n);
                this.buf = rest;
            }

            if n == 0 {
                return Poll::Ready(Err(zero_write()));
            }
        }

        Poll::Ready(Ok(()))
    }
}
