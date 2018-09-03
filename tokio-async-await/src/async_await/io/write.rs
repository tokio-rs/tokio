use tokio_io::AsyncWrite;

use futures_core::future::Future;
use futures_core::task::{self, Poll};

use std::io;
use std::marker::Unpin;
use std::pin::PinMut;

/// A future used to write data.
#[derive(Debug)]
pub struct Write<'a, T: 'a + ?Sized> {
    writer: &'a mut T,
    buf: &'a [u8],
}

// Pinning is never projected to fields
impl<'a, T: ?Sized> Unpin for Write<'a, T> {}

impl<'a, T: AsyncWrite + ?Sized> Write<'a, T> {
    pub(super) fn new(writer: &'a mut T, buf: &'a [u8]) -> Write<'a, T> {
        Write {
            writer,
            buf,
        }
    }
}

impl<'a, T: AsyncWrite + ?Sized> Future for Write<'a, T> {
    type Output = io::Result<usize>;

    fn poll(mut self: PinMut<Self>, _cx: &mut task::Context) -> Poll<io::Result<usize>> {
        use crate::async_await::compat::forward::convert_poll;

        let this = &mut *self;
        convert_poll(this.writer.poll_write(this.buf))
    }
}
