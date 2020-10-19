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
        _pin: PhantomPinned,
    }
}

pub(super) fn read_to_end_internal<R: AsyncRead + ?Sized>(
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
fn poll_read_to_end<R: AsyncRead + ?Sized>(
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
    reserve(buf, 32);

    let mut unused_capacity = ReadBuf::uninit(get_unused_capacity(buf));

    ready!(read.poll_read(cx, &mut unused_capacity))?;

    let n = unused_capacity.filled().len();
    let new_len = buf.len() + n;

    // This should no longer even be possible in safe Rust. An implementor
    // would need to have unsafely *replaced* the buffer inside `ReadBuf`,
    // which... yolo?
    assert!(new_len <= buf.capacity());
    unsafe {
        buf.set_len(new_len);
    }
    Poll::Ready(Ok(n))
}

/// Allocates more memory and ensures that the unused capacity is prepared for use
/// with the `AsyncRead`.
fn reserve(buf: &mut Vec<u8>, bytes: usize) {
    if buf.capacity() - buf.len() >= bytes {
        return;
    }
    buf.reserve(bytes);
}

/// Returns the unused capacity of the provided vector.
fn get_unused_capacity(buf: &mut Vec<u8>) -> &mut [MaybeUninit<u8>] {
    let uninit = bytes::BufMut::bytes_mut(buf);
    unsafe { &mut *(uninit as *mut _ as *mut [MaybeUninit<u8>]) }
}

impl<A> Future for ReadToEnd<'_, A>
where
    A: AsyncRead + ?Sized + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        read_to_end_internal(me.buf, Pin::new(*me.reader), me.read, cx)
    }
}
