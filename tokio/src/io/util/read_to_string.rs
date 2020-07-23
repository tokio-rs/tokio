use crate::io::util::read_line::finish_string_read;
use crate::io::util::read_to_end::{prepare_buffer, read_to_end_internal};
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
        /// This is the buffer we were provided. It will be replaced with an empty string
        /// while reading to postpone utf-8 handling until after reading.
        output: &'a mut String,
        /// The actual allocation of the string is moved into this vector instead.
        buf: Vec<u8>,
        /// The number of bytes appended to buf. This can be less than buf.len() if
        /// the buffer was not empty when the operation was started.
        read: usize,
    }
}

pub(crate) fn read_to_string<'a, R>(
    reader: &'a mut R,
    string: &'a mut String,
) -> ReadToString<'a, R>
where
    R: AsyncRead + ?Sized + Unpin,
{
    let mut buf = mem::replace(string, String::new()).into_bytes();
    prepare_buffer(&mut buf, reader);
    ReadToString {
        reader,
        buf,
        output: string,
        read: 0,
    }
}

/// # Safety
///
/// Before first calling this method, the unused capacity must have been
/// prepared for use with the provided AsyncRead. This can be done using the
/// `prepare_buffer` function in `read_to_end.rs`.
unsafe fn read_to_string_internal<R: AsyncRead + ?Sized>(
    reader: Pin<&mut R>,
    output: &mut String,
    buf: &mut Vec<u8>,
    read: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<io::Result<usize>> {
    let io_res = ready!(read_to_end_internal(buf, reader, read, cx));
    let utf8_res = String::from_utf8(mem::replace(buf, Vec::new()));

    // At this point both buf and output are empty. The allocation is in utf8_res.

    debug_assert!(buf.is_empty());
    debug_assert!(output.is_empty());
    finish_string_read(io_res, utf8_res, *read, output, true)
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
            output,
            read,
        } = &mut *self;

        // safety: The constructor of ReadToString called `prepare_buffer`.
        unsafe { read_to_string_internal(Pin::new(*reader), output, buf, read, cx) }
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
