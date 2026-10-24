use crate::io::util::read_until::read_until_internal;
use crate::io::AsyncBufRead;

use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::mem;
use std::pin::Pin;
use std::str;
use std::string::FromUtf8Error;
use std::task::{ready, Context, Poll};

pin_project! {
    /// Future for the [`read_line`](crate::io::AsyncBufReadExt::read_line) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadLine<'a, R: ?Sized> {
        reader: &'a mut R,
        // This is the buffer we were provided. It will be replaced with an empty string
        // while reading to postpone utf-8 handling until after reading.
        output: &'a mut String,
        // The actual allocation of the string is moved into this vector instead.
        buf: Vec<u8>,
        // The number of bytes appended to buf. This can be less than buf.len() if
        // the buffer was not empty when the operation was started.
        read: usize,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

pub(crate) fn read_line<'a, R>(reader: &'a mut R, string: &'a mut String) -> ReadLine<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    ReadLine {
        reader,
        buf: mem::take(string).into_bytes(),
        output: string,
        read: 0,
        _pin: PhantomPinned,
    }
}

fn put_back_original_data(output: &mut String, mut vector: Vec<u8>, num_bytes_read: usize) {
    let original_len = vector.len() - num_bytes_read;
    vector.truncate(original_len);
    *output = String::from_utf8(vector).expect("The original data must be valid utf-8.");
}

/// Converts `vector` into a `String`, given that all but its last `appended`
/// bytes came from a `String` and are therefore valid UTF-8 already.
fn into_string(vector: Vec<u8>, appended: usize) -> Result<String, FromUtf8Error> {
    if str::from_utf8(&vector[vector.len() - appended..]).is_ok() {
        // Safety: the concatenation of two valid UTF-8 sequences is valid UTF-8.
        Ok(unsafe { String::from_utf8_unchecked(vector) })
    } else {
        // Validate everything so that the error reports its position in the
        // whole string.
        String::from_utf8(vector)
    }
}

/// This handles the various failure cases and puts the string back into `output`.
///
/// The `truncate_on_io_error` `bool` is necessary because `read_to_string` and `read_line`
/// disagree on what should happen when an IO error occurs.
pub(super) fn finish_string_read(
    io_res: io::Result<usize>,
    vector: Vec<u8>,
    read: usize,
    output: &mut String,
    truncate_on_io_error: bool,
) -> Poll<io::Result<usize>> {
    let appended = *io_res.as_ref().unwrap_or(&read);
    let utf8_res = into_string(vector, appended);

    match (io_res, utf8_res) {
        (Ok(num_bytes), Ok(string)) => {
            debug_assert_eq!(read, 0);
            *output = string;
            Poll::Ready(Ok(num_bytes))
        }
        (Err(io_err), Ok(string)) => {
            *output = string;
            if truncate_on_io_error {
                let original_len = output.len() - read;
                output.truncate(original_len);
            }
            Poll::Ready(Err(io_err))
        }
        (Ok(num_bytes), Err(utf8_err)) => {
            debug_assert_eq!(read, 0);
            let utf8_error = utf8_err.utf8_error();
            put_back_original_data(output, utf8_err.into_bytes(), num_bytes);

            Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidData, utf8_error)))
        }
        (Err(io_err), Err(utf8_err)) => {
            put_back_original_data(output, utf8_err.into_bytes(), read);

            Poll::Ready(Err(io_err))
        }
    }
}

fn read_line_internal<R: AsyncBufRead + ?Sized>(
    reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    output: &mut String,
    buf: &mut Vec<u8>,
    read: &mut usize,
) -> Poll<io::Result<usize>> {
    let io_res = ready!(read_until_internal(reader, cx, b'\n', buf, read));

    debug_assert!(output.is_empty());
    finish_string_read(io_res, mem::take(buf), *read, output, false)
}

impl<R: AsyncBufRead + ?Sized + Unpin> Future for ReadLine<'_, R> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        read_line_internal(Pin::new(*me.reader), cx, me.output, me.buf, me.read)
    }
}
