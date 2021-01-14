use crate::io::util::read_line::finish_string_read;
use crate::io::util::read_to_end::read_to_end_internal;
use crate::io::AsyncRead;

use pin_project_lite::pin_project;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{io, mem};

pin_project! {
    /// Future for the [`read_to_string`](super::AsyncReadExt::read_to_string) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadToString<'a, R: ?Sized> {
        reader: &'a mut R,
        // This is the buffer we were provided. It will be replaced with an empty string
        // while reading to postpone utf-8 handling until after reading.
        output: &'a mut String,
        // The actual allocation of the string is moved into this vector instead.
        buf: Vec<u8>,
        // The number of bytes appended to buf. This can be less than buf.len() if
        // the buffer was not empty when the operation was started.
        read: usize,
        // The number of initialized bytes in the unused capacity of the vector.
        // Always between 0 and `buf.capacity() - buf.len()`.
        initialized: usize,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

pub(crate) fn read_to_string<'a, R>(
    reader: &'a mut R,
    string: &'a mut String,
) -> ReadToString<'a, R>
where
    R: AsyncRead + ?Sized + Unpin,
{
    let buf = mem::replace(string, String::new()).into_bytes();
    ReadToString {
        reader,
        buf,
        output: string,
        read: 0,
        initialized: 0,
        _pin: PhantomPinned,
    }
}

/// # Safety
///
/// The first `num_initialized` bytes of the unused capacity of the vector must be
/// initialized.
///
/// This function guarantees that the above invariant still holds when the function
/// returns.
unsafe fn read_to_string_internal<R: AsyncRead + ?Sized>(
    reader: Pin<&mut R>,
    output: &mut String,
    buf: &mut Vec<u8>,
    read: &mut usize,
    initialized: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<io::Result<usize>> {
    let io_res = ready!(read_to_end_internal(buf, reader, read, initialized, cx));
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        // Safety: initialized is initially zero, which is always correct, and
        // `read_to_string_internal` guarantees that its invariant still holds
        // when it returns.
        unsafe {
            read_to_string_internal(
                Pin::new(*me.reader),
                me.output,
                me.buf,
                me.read,
                me.initialized,
                cx,
            )
        }
    }
}
