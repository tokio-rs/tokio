use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_io::AsyncRead;

/// Tries to read some bytes directly into the given `buf` in asynchronous
/// manner, returning a future type.
///
/// The returned future will resolve to both the I/O stream and the buffer
/// as well as the number of bytes read once the read operation is completed.
pub fn read<'a, R>(reader: &'a mut R, buf: &'a mut [u8]) -> Read<'a, R>
where
    R: AsyncRead + Unpin + ?Sized,
{
    Read { reader, buf }
}

/// A future which can be used to easily read available number of bytes to fill
/// a buffer.
///
/// Created by the [`read`] function.
#[derive(Debug)]
pub struct Read<'a, R: ?Sized> {
    reader: &'a mut R,
    buf: &'a mut [u8],
}

// forward Unpin
impl<'a, R: Unpin + ?Sized> Unpin for Read<'_, R> {}

impl<R> Future for Read<'_, R>
where
    R: AsyncRead + Unpin + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = &mut *self;
        Pin::new(&mut *me.reader).poll_read(cx, me.buf)
    }
}
