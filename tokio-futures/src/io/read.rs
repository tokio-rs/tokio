use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{self, Poll};
use tokio_io::AsyncRead;

/// A future which can be used to read bytes.
#[derive(Debug)]
pub struct Read<'a, T: ?Sized> {
    reader: &'a mut T,
    buf: &'a mut [u8],
}

// Pinning is never projected to fields
impl<'a, T: ?Sized> Unpin for Read<'a, T> {}

impl<'a, T: AsyncRead + ?Sized> Read<'a, T> {
    pub(super) fn new(reader: &'a mut T, buf: &'a mut [u8]) -> Read<'a, T> {
        Read { reader, buf }
    }
}

impl<'a, T: AsyncRead + ?Sized> Future for Read<'a, T> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut task::Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;

        let this = &mut *self;
        convert_poll(this.reader.poll_read(this.buf))
    }
}
