use crate::io::AsyncRead;

use bytes::BufMut;
use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Tries to read some bytes directly into the given `buf` in asynchronous
/// manner, returning a future type.
///
/// The returned future will resolve to both the I/O stream and the buffer
/// as well as the number of bytes read once the read operation is completed.
pub(crate) fn read<'a, R, B>(reader: &'a mut R, buf: &'a mut B) -> Read<'a, R, B>
where
    R: AsyncRead + Unpin + ?Sized,
    B: BufMut + Unpin,
{
    Read { reader, buf }
}

/// A future which can be used to easily read available number of bytes to fill
/// a buffer.
///
/// Created by the [`read`] function.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Read<'a, R: ?Sized, B> {
    reader: &'a mut R,
    buf: &'a mut B,
}

impl<R, B> Future for Read<'_, R, B>
where
    R: AsyncRead + Unpin + ?Sized,
    B: BufMut + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = &mut *self;
        Pin::new(&mut *me.reader).poll_read(cx, me.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<Read<'_, PhantomPinned, PhantomPinned>>();
    }
}
