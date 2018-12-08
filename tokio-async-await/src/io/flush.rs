use tokio_io::AsyncWrite;


use std::io;
use std::future::Future;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{LocalWaker, Poll};

/// A future used to fully flush an I/O object.
#[derive(Debug)]
pub struct Flush<'a, T: ?Sized + 'a> {
    writer: &'a mut T,
}

// Pin is never projected to fields
impl<'a, T: ?Sized> Unpin for Flush<'a, T> {}

impl<'a, T: AsyncWrite + ?Sized> Flush<'a, T> {
    pub(super) fn new(writer: &'a mut T) -> Flush<'a, T> {
        Flush { writer }
    }
}

impl<'a, T: AsyncWrite + ?Sized> Future for Flush<'a, T> {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, _wx: &LocalWaker) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;
        convert_poll(self.writer.poll_flush())
    }
}
