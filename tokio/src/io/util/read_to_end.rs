use crate::io::{AsyncRead, ReadBuf};

use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::mem::{self, MaybeUninit};
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadToEnd<'a, R: ?Sized> {
        reader: &'a mut R,
        buf: &'a mut Vec<u8>,
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

pub(crate) fn read_to_end<'a, R>(reader: &'a mut R, buffer: &'a mut Vec<u8>) -> ReadToEnd<'a, R>
where
    R: AsyncRead + Unpin + ?Sized,
{
    ReadToEnd {
        reader,
        buf: buffer,
        read: 0,
        initialized: 0,
        _pin: PhantomPinned,
    }
}

/// # Safety:
///
/// The first `num_initialized` bytes of the unused capacity of the vector must be
/// initialized.
///
/// This function guarantees that the above invariant still holds when the function
/// returns.
pub(super) unsafe fn read_to_end_internal<R: AsyncRead + ?Sized>(
    buf: &mut Vec<u8>,
    mut reader: Pin<&mut R>,
    num_read: &mut usize,
    num_initialized: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<io::Result<usize>> {
    loop {
        // safety: The caller promised to prepare the buffer.
        let ret = ready!(poll_read_to_end(buf, reader.as_mut(), num_initialized, cx));
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
/// # Safety:
///
/// The first `num_initialized` bytes of the unused capacity of the vector must be
/// initialized.
///
/// This function guarantees that the above invariant still holds when the function
/// returns.
fn poll_read_to_end<R: AsyncRead + ?Sized>(
    buf: &mut Vec<u8>,
    read: Pin<&mut R>,
    num_initialized: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<io::Result<usize>> {
    // This uses an adaptive system to extend the vector when it fills. We want to
    // avoid paying to allocate and zero a huge chunk of memory if the reader only
    // has 4 bytes while still making large reads if the reader does have a ton
    // of data to return. Simply tacking on an extra DEFAULT_BUF_SIZE space every
    // time is 4,500 times (!) slower than this if the reader has a very small
    // amount of data to return.
    reserve(buf, 32, num_initialized);

    let mut unused_capacity = ReadBuf::uninit(get_unused_capacity(buf));
    unsafe {
        unused_capacity.assume_init(*num_initialized);
    }

    let ptr = unused_capacity.filled().as_ptr();
    // If poll_read returns Poll::Pending, the value of `num_initialized` is
    // still correct as the vector's length is unchanged, and `poll_read` cannot
    // de-initialize any bytes in `unused_capacity`.
    ready!(read.poll_read(cx, &mut unused_capacity))?;

    // Ensure that the pointer does not change from under us.
    if ptr != unused_capacity.filled().as_ptr() {
        *num_initialized = 0;
        panic!("The poll_read call has mem::swapped the ReadBuf.");
    }

    let n = unused_capacity.filled().len();
    let new_initialized = unused_capacity.initialized().len() - n;
    let new_len = buf.len() + n;

    // Safety: The safety invariants of ReadBuf guarantees that the new bytes have
    // been initialized.
    assert!(new_len <= buf.capacity());
    unsafe {
        *num_initialized = new_initialized;
        buf.set_len(new_len);
    }

    Poll::Ready(Ok(n))
}

/// Allocates more memory and ensures that the unused capacity is prepared for use
/// with the `AsyncRead`.
fn reserve(buf: &mut Vec<u8>, bytes: usize, num_initialized: &mut usize) {
    if buf.capacity() - buf.len() >= bytes {
        return;
    }
    *num_initialized = 0;
    buf.reserve(bytes);
}

/// Returns the unused capacity of the provided vector.
fn get_unused_capacity(buf: &mut Vec<u8>) -> &mut [MaybeUninit<u8>] {
    let uninit = bytes::BufMut::chunk_mut(buf);
    unsafe { &mut *(uninit as *mut _ as *mut [MaybeUninit<u8>]) }
}

impl<A> Future for ReadToEnd<'_, A>
where
    A: AsyncRead + ?Sized + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        // Safety: initialized is initially zero, which is always correct, and
        // `read_to_string_internal` guarantees that its invariant still holds
        // when it returns.
        unsafe { read_to_end_internal(me.buf, Pin::new(*me.reader), me.read, me.initialized, cx) }
    }
}
