use crate::io::AsyncRead;

use bytes::BufMut;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) fn read_buf<'a, R, B>(reader: &'a mut R, buf: &'a mut B) -> ReadBuf<'a, R, B>
where
    R: AsyncRead,
    B: BufMut,
{
    ReadBuf { reader, buf }
}

cfg_io_util! {
    /// Future returned by [`read_buf`](AsyncReadExt::read_buf).
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ReadBuf<'a, R, B> {
        reader: &'a mut R,
        buf: &'a mut B,
    }
}

impl<R, B> Future for ReadBuf<'_, R, B>
where
    R: AsyncRead,
    B: BufMut,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        // safety: no data is moved from self
        unsafe {
            let me = self.get_unchecked_mut();
            Pin::new_unchecked(&mut *me.reader).poll_read_buf(cx, &mut me.buf)
        }
    }
}
