use crate::io::AsyncRead;

use std::future::Future;
use std::io;
use std::mem::{self, MaybeUninit};
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
pub struct ReadToEnd<'a, R: ?Sized> {
    reader: &'a mut R,
    buf: &'a mut Vec<u8>,
    /// The number of bytes appended to buf. This can be less than buf.len() if
    /// the buffer was not empty when the operation was started.
    read: usize,
}

pub(crate) fn read_to_end<'a, R>(reader: &'a mut R, buffer: &'a mut Vec<u8>) -> ReadToEnd<'a, R>
where
    R: AsyncRead + Unpin + ?Sized,
{
    prepare_buffer(buffer, reader);
    ReadToEnd {
        reader,
        buf: buffer,
        read: 0,
    }
}

/// # Safety
///
/// Before first calling this method, the unused capacity must have been
/// prepared for use with the provided AsyncRead. This can be done using the
/// `prepare_buffer` function later in this file.
pub(super) unsafe fn read_to_end_internal<R: AsyncRead + ?Sized>(
    buf: &mut Vec<u8>,
    mut reader: Pin<&mut R>,
    num_read: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<io::Result<usize>> {
    loop {
        // safety: The caller promised to prepare the buffer.
        let ret = ready!(poll_read_to_end(buf, reader.as_mut(), cx));
        match ret {
            Err(err) => return Poll::Ready(Err(err)),
            Ok(0) => return Poll::Ready(Ok(mem::replace(num_read, 0))),
            Ok(num) => {
                *num_read += num;
            }
        }
    }
}

/// Tries to read from the provided AsyncRead.
///
/// The length of the buffer is increased by the number of bytes read.
///
/// # Safety
///
/// The caller ensures that the buffer has been prepared for use with the
/// AsyncRead before calling this function. This can be done using the
/// `prepare_buffer` function later in this file.
unsafe fn poll_read_to_end<R: AsyncRead + ?Sized>(
    buf: &mut Vec<u8>,
    read: Pin<&mut R>,
    cx: &mut Context<'_>,
) -> Poll<io::Result<usize>> {
    // This uses an adaptive system to extend the vector when it fills. We want to
    // avoid paying to allocate and zero a huge chunk of memory if the reader only
    // has 4 bytes while still making large reads if the reader does have a ton
    // of data to return. Simply tacking on an extra DEFAULT_BUF_SIZE space every
    // time is 4,500 times (!) slower than this if the reader has a very small
    // amount of data to return.
    reserve(buf, &*read, 32);

    let unused_capacity: &mut [MaybeUninit<u8>] = get_unused_capacity(buf);

    // safety: The buffer has been prepared for use with the AsyncRead before
    // calling this function.
    let slice: &mut [u8] = &mut *(unused_capacity as *mut [MaybeUninit<u8>] as *mut [u8]);

    let res = ready!(read.poll_read(cx, slice));
    if let Ok(num) = res {
        // safety: There are two situations:
        //
        // 1. The AsyncRead has not overriden `prepare_uninitialized_buffer`.
        //
        // In this situation, the default implementation of that method will have
        // zeroed the unused capacity. This means that setting the length will
        // never expose uninitialized memory in the vector.
        //
        // Note that the assert! below ensures that we don't set the length to
        // something larger than the capacity, which malicious implementors might
        // try to have us do.
        //
        // 2. The AsyncRead has overriden `prepare_uninitialized_buffer`.
        //
        // In this case, the safety of the `set_len` call below relies on this
        // guarantee from the documentation on `prepare_uninitialized_buffer`:
        //
        // > This function isn't actually unsafe to call but unsafe to implement.
        // > The implementer must ensure that either the whole buf has been zeroed
        // > or poll_read() overwrites the buffer without reading it and returns
        // > correct value.
        //
        // Note that `prepare_uninitialized_buffer` is unsafe to implement, so this
        // is a guarantee we can rely on in unsafe code.
        //
        // The assert!() is technically only necessary in the first case.
        let new_len = buf.len() + num;
        assert!(new_len <= buf.capacity());

        buf.set_len(new_len);
    }
    Poll::Ready(res)
}

/// This function prepares the unused capacity for use with the provided AsyncRead.
pub(super) fn prepare_buffer<R: AsyncRead + ?Sized>(buf: &mut Vec<u8>, read: &R) {
    let buffer = get_unused_capacity(buf);

    // safety: This function is only unsafe to implement.
    unsafe {
        read.prepare_uninitialized_buffer(buffer);
    }
}

/// Allocates more memory and ensures that the unused capacity is prepared for use
/// with the `AsyncRead`.
fn reserve<R: AsyncRead + ?Sized>(buf: &mut Vec<u8>, read: &R, bytes: usize) {
    if buf.capacity() - buf.len() >= bytes {
        return;
    }
    buf.reserve(bytes);
    // The call above has reallocated the buffer, so we must reinitialize the entire
    // unused capacity, even if we already initialized some of it before the resize.
    prepare_buffer(buf, read);
}

/// Returns the unused capacity of the provided vector.
fn get_unused_capacity(buf: &mut Vec<u8>) -> &mut [MaybeUninit<u8>] {
    bytes::BufMut::bytes_mut(buf)
}

impl<A> Future for ReadToEnd<'_, A>
where
    A: AsyncRead + ?Sized + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { reader, buf, read } = &mut *self;

        // safety: The constructor of ReadToEnd calls `prepare_buffer`
        unsafe { read_to_end_internal(buf, Pin::new(*reader), read, cx) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<ReadToEnd<'_, PhantomPinned>>();
    }
}
