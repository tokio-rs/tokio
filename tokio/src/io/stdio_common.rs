//! Contains utilities for stdout and stderr.
use crate::io::AsyncWrite;
use std::pin::Pin;
use std::task::{Context, Poll};
/// # Windows
/// AsyncWrite adapter that finds last char boundary in given buffer and does not write the rest,
/// if buffer contents seems to be utf8. Otherwise it only trims buffer down to MAX_BUF.
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
    //#[cfg_attr(not(any(target_os = "windows", test)), allow(unreachable_code))]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        // just a closure to avoid repetitive code
        let mut call_inner = move |buf| Pin::new(&mut self.inner).poll_write(cx, buf);

        // 1. Only windows stdio can suffer from non-utf8.
        // We also check for `test` so that we can write some tests
        // for further code. Since `AsyncWrite` can always shrink
        // buffer at its discretion, excessive (i.e. in tests) shrinking
        // does not break correctness.
        // 2. If buffer is small, it will not be shrinked.
        // That's why, it's "textness" will not change, so we don't have
        // to fixup it.
        if cfg!(not(any(target_os = "windows", test))) || buf.len() <= crate::io::blocking::MAX_BUF
        {
            return call_inner(buf);
        }

        buf = &buf[..crate::io::blocking::MAX_BUF];

        // Now there are two possibilites.
        // If caller gave is binary buffer, we **should not** shrink it
        // anymore, because excessive shrinking hits performance.
        // If caller gave as binary buffer, we  **must** additionaly
        // shrink it to strip incomplete char at the end of buffer.
        // that's why check we will perform now is allowed to have
        // false-positive.

        // this constant is defined by Unicode standard.
        const MAX_BYTES_PER_CHAR: usize = 4;

        // Subject for tweaking here
        const MAGIC_CONST: usize = 8;

        // Now let's look at the first MAX_BYTES_PER_CHAR * MAGIC_CONST bytes.
        // if they are (possibly incomplete) utf8, then we can be quite sure
        // that input buffer was utf8.

        let have_to_fix_up = match std::str::from_utf8(&buf[..MAX_BYTES_PER_CHAR * MAGIC_CONST]) {
            Ok(_) => {
                // We do not need to shrink this buffer:
                // we were lucky enough and it didn't broke
                // during shrinking
                false
            }
            Err(err) => {
                let bad_bytes = buf.len() - err.valid_up_to();
                // this must hold for any shrinked utf8 buffer.
                bad_bytes < MAX_BYTES_PER_CHAR
            }
        };

        if have_to_fix_up {
            // We must pop several bytes at the end which form incomplete
            // character. To achieve it, we exploit UTF8 encoding:
            // for any code point, all bytes except first start with 0b10 prefix.
            // see https://en.wikipedia.org/wiki/UTF-8#Encoding for details
            let trailing_incomplete_char_size = buf
                .iter()
                .rev()
                .position(|byte| *byte < 0b1000_0000 || *byte >= 0b1100_0000)
                .unwrap();
            buf = &buf[..buf.len() - trailing_incomplete_char_size];
        }

        return call_inner(buf);
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
