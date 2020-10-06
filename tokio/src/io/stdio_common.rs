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

// this constant is defined by Unicode standard.
const MAX_BYTES_PER_CHAR: usize = 4;

// Subject for tweaking here
const MAGIC_CONST: usize = 8;

impl<W> crate::io::AsyncWrite for SplitByUtf8BoundaryIfWindows<W>
where
    W: AsyncWrite + Unpin,
{
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

        // Now let's look at the first MAX_BYTES_PER_CHAR * MAGIC_CONST bytes.
        // if they are (possibly incomplete) utf8, then we can be quite sure
        // that input buffer was utf8.

        let have_to_fix_up = match std::str::from_utf8(&buf[..MAX_BYTES_PER_CHAR * MAGIC_CONST]) {
            Ok(_) => true,
            Err(err) => {
                let incomplete_bytes = MAX_BYTES_PER_CHAR * MAGIC_CONST - err.valid_up_to();
                incomplete_bytes < MAX_BYTES_PER_CHAR
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
                .take(MAX_BYTES_PER_CHAR)
                .position(|byte| *byte < 0b1000_0000 || *byte >= 0b1100_0000)
                .unwrap_or(0)
                + 1;
            buf = &buf[..buf.len() - trailing_incomplete_char_size];
        }

        call_inner(buf)
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

    struct TextMockWriter;

    impl crate::io::AsyncWrite for TextMockWriter {
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

    struct LoggingMockWriter {
        write_history: Vec<usize>,
    }

    impl LoggingMockWriter {
        fn new() -> Self {
            LoggingMockWriter {
                write_history: Vec::new(),
            }
        }
    }

    impl crate::io::AsyncWrite for LoggingMockWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            assert!(buf.len() <= MAX_BUF);
            self.write_history.push(buf.len());
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
        let mut wr = super::SplitByUtf8BoundaryIfWindows::new(TextMockWriter);
        let fut = async move {
            wr.write_all(data.as_bytes()).await.unwrap();
        };
        crate::runtime::Builder::new()
            .basic_scheduler()
            .build()
            .unwrap()
            .block_on(fut);
    }

    #[test]
    fn test_pseudo_text() {
        // In this test we write a piece of binary data, whose beginning is
        // text though. We then validate that even in this corner case buffer
        // was not shrinked too much.
        let checked_count = super::MAGIC_CONST * super::MAX_BYTES_PER_CHAR;
        let mut data: Vec<u8> = str::repeat("a", checked_count).into();
        data.extend(std::iter::repeat(0b1010_1010).take(MAX_BUF - checked_count + 1));
        let mut writer = LoggingMockWriter::new();
        let mut splitter = super::SplitByUtf8BoundaryIfWindows::new(&mut writer);
        crate::runtime::Builder::new()
            .basic_scheduler()
            .build()
            .unwrap()
            .block_on(async {
                splitter.write_all(&data).await.unwrap();
            });
        // Check that at most two writes were performed
        assert!(writer.write_history.len() <= 2);
        // Check that all has been written
        assert_eq!(
            writer.write_history.iter().copied().sum::<usize>(),
            data.len()
        );
        // Check that at most MAX_BYTES_PER_CHAR + 1 (i.e. 5) bytes were shrinked
        // from the buffer: one because it was outside of MAX_BUF boundary, and
        // up to one "utf8 code point".
        assert!(data.len() - writer.write_history[0] <= super::MAX_BYTES_PER_CHAR + 1);
    }
}
