use crate::io::AsyncRead;

use bytes::BufMut;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

pub(crate) fn read_buf_exact<'a, R, B>(reader: &'a mut R, buf: &'a mut B) -> ReadBufExact<'a, R, B>
where
    R: AsyncRead + Unpin + ?Sized,
    B: BufMut + ?Sized,
{
    ReadBufExact {
        reader,
        capacity: buf.remaining_mut(),
        buf,
        _pin: PhantomPinned,
    }
}

pin_project! {
    /// Creates a future which will read exactly enough bytes to fill `buf`,
    /// returning an error if EOF is hit sooner.
    ///
    /// On success the number of bytes is returned
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadBufExact<'a, R: ?Sized, B: ?Sized> {
        reader: &'a mut R,
        buf: &'a mut B,
        capacity: usize,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<R, B> Future for ReadBufExact<'_, R, B>
where
    R: AsyncRead + Unpin + ?Sized,
    B: BufMut + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        use crate::io::ReadBuf;
        use std::mem::MaybeUninit;

        let me = self.project();

        loop {
            if me.buf.has_remaining_mut() {
                let n = {
                    let dst = me.buf.chunk_mut();
                    let dst = unsafe { &mut *(dst as *mut _ as *mut [MaybeUninit<u8>]) };
                    let mut buf = ReadBuf::uninit(dst);
                    let ptr = buf.filled().as_ptr();
                    ready!(Pin::new(&mut *me.reader).poll_read(cx, &mut buf)?);

                    // Ensure the pointer does not change from under us
                    assert_eq!(ptr, buf.filled().as_ptr());
                    buf.filled().len()
                };

                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")).into();
                }

                // Safety: This is guaranteed to be the number of initialized (and read)
                // bytes due to the invariants provided by `ReadBuf::filled`.
                unsafe {
                    me.buf.advance_mut(n);
                }
            } else {
                return Poll::Ready(Ok(*me.capacity));
            }
        }
    }
}
