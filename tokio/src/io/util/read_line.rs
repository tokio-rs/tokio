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
        /// This is the buffer we were provided. It will be replaced with an empty string
        /// while reading to postpone utf-8 handling until after reading.
        output: &'a mut String,
        /// The actual allocation of the string is moved into a vector instead.
        buf: Vec<u8>,
        /// The number of bytes appended to buf. This can be less than buf.len() if
        /// the buffer was not empty when the operation was started.
        read: usize,
    }
}

pub(crate) fn read_line<'a, R>(reader: &'a mut R, string: &'a mut String) -> ReadLine<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    ReadLine {
        reader,
        buf: mem::replace(string, String::new()).into_bytes(),
        output: string,
        read: 0,
    }
}

fn put_back_original_data(output: &mut String, mut vector: Vec<u8>, num_bytes_read: usize) {
    let original_len = vector.len() - num_bytes_read;
    vector.truncate(original_len);
    *output = String::from_utf8(vector).expect("The original data must be valid utf-8.");
}

pub(super) fn read_line_internal<R: AsyncBufRead + ?Sized>(
    reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    output: &mut String,
    buf: &mut Vec<u8>,
    read: &mut usize,
) -> Poll<io::Result<usize>> {
    let io_res = ready!(read_until_internal(reader, cx, b'\n', buf, read));
    let utf8_res = String::from_utf8(mem::replace(buf, Vec::new()));

    // At this point both buf and output are empty. The allocation is in utf8_res.

    debug_assert!(buf.is_empty());
    match (io_res, utf8_res) {
        (Ok(num_bytes), Ok(string)) => {
            debug_assert_eq!(*read, 0);
            *output = string;
            Poll::Ready(Ok(num_bytes))
        }
        (Err(io_err), Ok(string)) => {
            *output = string;
            Poll::Ready(Err(io_err))
        }
        (Ok(num_bytes), Err(utf8_err)) => {
            debug_assert_eq!(*read, 0);
            put_back_original_data(output, utf8_err.into_bytes(), num_bytes);

            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )))
        }
        (Err(io_err), Err(utf8_err)) => {
            put_back_original_data(output, utf8_err.into_bytes(), *read);

            Poll::Ready(Err(io_err))
        }
    }
}

impl<R: AsyncBufRead + ?Sized + Unpin> Future for ReadLine<'_, R> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            reader,
            output,
            buf,
            read,
        } = &mut *self;

        read_line_internal(Pin::new(reader), cx, output, buf, read)
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
