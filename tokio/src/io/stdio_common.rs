//! Contains utilities for stdout and stderr.
use crate::io::AsyncWrite;
use std::pin::Pin;
use std::task::{Context, Poll};
/// # Windows
/// AsyncWrite adapter that finds last char boundary in given buffer and does not write the rest.
/// That's why, wrapped writer will always receive well-formed utf-8 bytes.
/// # Other platforms
/// passes data to `inner` as is
#[derive(Debug)]
pub(crate) struct SplitByUtf8BoundaryIfWindows<W> {
    inner: W,
}

impl<W> SplitByUtf8BoundaryIfWindows<W> {
    pub(crate) fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W> crate::io::AsyncWrite for SplitByUtf8BoundaryIfWindows<W>
where
    W: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        // following two ifs are enabled only on windows targets, because
        // on other targets we do not have problems with incomplete utf8 chars

        // ensure buffer is not longer than MAX_BUF
        #[cfg(target_os = "windows")]
        let buf = if buf.len() > crate::io::blocking::MAX_BUF {
            &buf[..crate::io::blocking::MAX_BUF]
        } else {
            buf
        };
        // now remove possible trailing incomplete character
        #[cfg(target_os = "windows")]
        let buf = match std::str::from_utf8(buf) {
            // `buf` is already utf-8, no need to trim it
            Ok(_) => buf,
            Err(err) if err.valid_up_to() == 0 => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "provided buffer does not contain utf-8 data",
                )));
            }
            Err(err) => &buf[..err.valid_up_to()],
        };
        // now pass trimmed input buffer to inner writer
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
