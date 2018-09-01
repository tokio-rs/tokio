use tokio_io::AsyncRead;

use futures_core::future::Future;
use futures_core::task::{self, Poll};
use futures_util::try_ready;

use std::io;
use std::marker::Unpin;
use std::mem::{self, PinMut};

/// A future which can be used to read exactly enough bytes to fill a buffer.
#[derive(Debug)]
pub struct ReadExact<'a, T: ?Sized + 'a> {
    reader: &'a mut T,
    buf: &'a mut [u8],
}

// Pinning is never projected to fields
impl<'a, T: ?Sized> Unpin for ReadExact<'a, T> {}

impl<'a, T: AsyncRead + ?Sized> ReadExact<'a, T> {
    pub(super) fn new(reader: &'a mut T, buf: &'a mut [u8]) -> ReadExact<'a, T> {
        ReadExact {
            reader,
            buf,
        }
    }
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")
}

impl<'a, T: AsyncRead + ?Sized> Future for ReadExact<'a, T> {
    type Output = io::Result<()>;

    fn poll(mut self: PinMut<Self>, _cx: &mut task::Context) -> Poll<Self::Output> {
        use crate::async_await::compat::forward::convert_poll;

        let this = &mut *self;

        while !this.buf.is_empty() {
            let n = try_ready!(convert_poll(this.reader.poll_read(this.buf)));

            {
                let (_, rest) = mem::replace(&mut this.buf, &mut []).split_at_mut(n);
                this.buf = rest;
            }
            if n == 0 {
                return Poll::Ready(Err(eof()))
            }
        }

        Poll::Ready(Ok(()))
    }
}
