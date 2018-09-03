use tokio_io::AsyncWrite;

use futures_core::future::Future;
use futures_core::task::{self, Poll};

use std::io;
use std::marker::Unpin;
use std::pin::PinMut;

/// A future used to fully flush an I/O object.
#[derive(Debug)]
pub struct Flush<'a, T: ?Sized + 'a> {
    writer: &'a mut T,
}

// PinMut is never projected to fields
impl<'a, T: ?Sized> Unpin for Flush<'a, T> {}

impl<'a, T: AsyncWrite + ?Sized> Flush<'a, T> {
    pub(super) fn new(writer: &'a mut T) -> Flush<'a, T> {
        Flush { writer }
    }
}

impl<'a, T: AsyncWrite + ?Sized> Future for Flush<'a, T> {
    type Output = io::Result<()>;

    fn poll(mut self: PinMut<Self>, _cx: &mut task::Context) -> Poll<Self::Output> {
        use crate::async_await::compat::forward::convert_poll;
        convert_poll(self.writer.poll_flush())
    }
}
