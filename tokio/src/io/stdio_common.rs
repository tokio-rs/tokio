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
        #[cfg(any(target_os = "windows", test))]
        let buf = if buf.len() > crate::io::blocking::MAX_BUF {
            &buf[..crate::io::blocking::MAX_BUF]
        } else {
            buf
        };
        // now remove possible trailing incomplete character
        #[cfg(any(target_os = "windows", test))]
        let buf = match std::str::from_utf8(buf) {
            // `buf` is already utf-8, no need to trim it futher
            Ok(_) => buf,
            Err(err) => {
                let bad_bytes = buf.len() - err.valid_up_to();
                // TODO: this is too conservative
                const MAX_BYTES_PER_CHAR: usize = 8;

                if bad_bytes <= MAX_BYTES_PER_CHAR && err.valid_up_to() > 0 {
                    // Input data is probably UTF-8, but last char was split
                    // after trimming.
                    // let's exclude this character from the buf
                    &buf[..err.valid_up_to()]
                } else {
                    // UTF-8 violation could not be caused by trimming.
                    // Let's pass buffer to underlying writer as is.
                    // Why do not we return error here? It is possible
                    // that stdout is not console. Such streams allow
                    // non-utf8 data. That's why, let's defer to underlying
                    // writer and let it return error if needed
                    buf
                }
            }
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

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use crate::io::AsyncWriteExt;
    use std::io;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;

    const MAX_BUF: usize = 16 * 1024;
    struct MockWriter;
    impl crate::io::AsyncWrite for MockWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            assert!(buf.len() <= MAX_BUF);
            assert!(std::str::from_utf8(buf).is_ok());
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn test_splitter() {
        let data = str::repeat("█", MAX_BUF);
        let mut wr = super::SplitByUtf8BoundaryIfWindows::new(MockWriter);
        let fut = async move {
            wr.write_all(data.as_bytes()).await.unwrap();
        };
        crate::runtime::Builder::new()
            .basic_scheduler()
            .build()
            .unwrap()
            .block_on(fut);
    }
}
