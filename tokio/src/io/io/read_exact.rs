use crate::io::AsyncRead;

use bytes::BufMut;
use futures_core::ready;
use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future which can be used to easily read exactly enough bytes to fill
/// a buffer.
///
/// Created by the [`AsyncRead::read_exact`].
pub(crate) fn read_exact<'a, A, B>(reader: &'a mut A, buf: &'a mut B) -> ReadExact<'a, A, B>
where
    A: AsyncRead + Unpin + ?Sized,
    B: BufMut + Unpin,
{
    ReadExact { reader, buf }
}

/// Creates a future which will read exactly enough bytes to fill `buf`,
/// returning an error if EOF is hit sooner.
///
/// On success the number of bytes is returned
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReadExact<'a, A: ?Sized, B> {
    reader: &'a mut A,
    buf: &'a mut B,
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")
}

impl<A, B> Future for ReadExact<'_, A, B>
where
    A: AsyncRead + Unpin + ?Sized,
    B: BufMut + Unpin,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = &mut *self;
        while me.buf.has_remaining_mut() {
            let n = ready!(Pin::new(&mut *me.reader).poll_read(cx, me.buf))?;
            if n == 0 {
                return Err(eof()).into();
            }
        }

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<ReadExact<'_, PhantomPinned, PhantomPinned>>();
    }
}
