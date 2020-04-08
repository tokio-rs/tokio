use crate::io::util::read_to_end::read_to_end_internal;
use crate::io::AsyncRead;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{io, mem};

cfg_io_util! {
    /// Future for the [`read_to_string`](super::AsyncReadExt::read_to_string) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadToString<'a, R: ?Sized> {
        reader: &'a mut R,
        buf: &'a mut String,
        bytes: Vec<u8>,
        start_len: usize,
    }
}

pub(crate) fn read_to_string<'a, R>(reader: &'a mut R, buf: &'a mut String) -> ReadToString<'a, R>
where
    R: AsyncRead + ?Sized + Unpin,
{
    let start_len = buf.len();
    ReadToString {
        reader,
        bytes: mem::replace(buf, String::new()).into_bytes(),
        buf,
        start_len,
    }
}

fn read_to_string_internal<R: AsyncRead + ?Sized>(
    reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut String,
    bytes: &mut Vec<u8>,
    start_len: usize,
) -> Poll<io::Result<usize>> {
    let ret = ready!(read_to_end_internal(reader, cx, bytes, start_len));
    if let Ok(string) = String::from_utf8(mem::take(bytes)) {
        debug_assert!(buf.is_empty());
        *bytes = mem::replace(buf, string).into_bytes();
        Poll::Ready(ret)
    } else {
        Poll::Ready(ret.and_then(|_| {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            ))
        }))
    }
}

impl<A> Future for ReadToString<'_, A>
where
    A: AsyncRead + ?Sized + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            reader,
            buf,
            bytes,
            start_len,
        } = &mut *self;
        read_to_string_internal(Pin::new(reader), cx, buf, bytes, *start_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<ReadToString<'_, PhantomPinned>>();
    }
}
