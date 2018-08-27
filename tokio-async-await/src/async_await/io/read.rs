use tokio_io::AsyncRead;

use futures_core::future::Future;
use futures_core::task::{self, Poll};

use std::io;
use std::marker::Unpin;
use std::mem::PinMut;

/// A future which can be used to read bytes.
#[derive(Debug)]
pub struct Read<'a, T: ?Sized + 'a> {
    reader: &'a mut T,
    buf: &'a mut [u8],
}

// Pinning is never projected to fields
impl<'a, T: ?Sized> Unpin for Read<'a, T> {}

impl<'a, T: AsyncRead + ?Sized> Read<'a, T> {
    pub(super) fn new(reader: &'a mut T, buf: &'a mut [u8]) -> Read<'a, T> {
        Read {
            reader,
            buf,
        }
    }
}

impl<'a, T: AsyncRead + ?Sized> Future for Read<'a, T> {
    type Output = io::Result<usize>;

    fn poll(mut self: PinMut<Self>, _cx: &mut task::Context) -> Poll<Self::Output> {
        use crate::async_await::compat::forward::convert_poll;

        let this = &mut *self;
        convert_poll(this.reader.poll_read(this.buf))
    }
}
