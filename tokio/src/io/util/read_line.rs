use crate::io::util::read_until::read_until_internal;
use crate::io::AsyncBufRead;

use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    /// Future for the [`read_line`](crate::io::AsyncBufReadExt::read_line) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadLine<'a, R: ?Sized> {
        reader: &'a mut R,
        buf: &'a mut String,
        bytes: Vec<u8>,
        read: usize,
    }
}

pub(crate) fn read_line<'a, R>(reader: &'a mut R, buf: &'a mut String) -> ReadLine<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    ReadLine {
        reader,
        bytes: mem::replace(buf, String::new()).into_bytes(),
        buf,
        read: 0,
    }
}

pub(super) fn read_line_internal<R: AsyncBufRead + ?Sized>(
    reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut String,
    bytes: &mut Vec<u8>,
    read: &mut usize,
) -> Poll<io::Result<usize>> {
    let ret = ready!(read_until_internal(reader, cx, b'\n', bytes, read))?;
    match String::from_utf8(mem::replace(bytes, Vec::new())) {
        Ok(string) => {
            debug_assert!(buf.is_empty());
            *buf = string;
            Poll::Ready(Ok(ret))
        }
        Err(e) => {
            *bytes = e.into_bytes();
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )))
        }
    }
}

impl<R: AsyncBufRead + ?Sized + Unpin> Future for ReadLine<'_, R> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            reader,
            buf,
            bytes,
            read,
        } = &mut *self;
        let ret = ready!(read_line_internal(Pin::new(reader), cx, buf, bytes, read));
        if ret.is_err() {
            // If an error occurred, put the data back, but only if it is valid utf-8.
            if let Ok(string) = String::from_utf8(mem::replace(bytes, Vec::new())) {
                **buf = string;
            }
        }
        Poll::Ready(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<ReadLine<'_, PhantomPinned>>();
    }
}
