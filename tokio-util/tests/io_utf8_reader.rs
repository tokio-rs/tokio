use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures::ready;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, ReadBuf};
use tokio_test::{assert_ok, assert_pending, assert_ready_ok, task};
use tokio_util::io::Utf8Reader;

const EMPTY: &[u8] = b"";
const ONE_BYTE_UTF8: &[u8] = b"test string";
// "test string" Google-translated to Arabic
const TWO_BYTE_UTF8: &[u8] = "سلسلة الاختبار".as_bytes();
// "test string" Google-translated to Chinese
const THREE_BYTE_UTF8: &[u8] = "测试字符串".as_bytes();
const FOUR_BYTE_UTF8: &[u8] = "😀😬😁😂😃".as_bytes();

#[derive(Debug)]
struct MaybePending<'a> {
    inner: &'a [u8],
    ready_read: bool,
    ready_fill_buf: bool,
}

impl<'a> MaybePending<'a> {
    fn new(inner: &'a [u8]) -> Self {
        Self {
            inner,
            ready_read: false,
            ready_fill_buf: false,
        }
    }
}

impl AsyncRead for MaybePending<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.ready_read {
            self.ready_read = false;
            Pin::new(&mut self.inner).poll_read(cx, buf)
        } else {
            self.ready_read = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl AsyncBufRead for MaybePending<'_> {
    fn poll_fill_buf(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        if self.ready_fill_buf {
            self.ready_fill_buf = false;
            Pin::new(&mut self.get_mut().inner).poll_fill_buf(cx)
        } else {
            self.ready_fill_buf = true;
            Poll::Pending
        }
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.inner = &self.inner[amt..];
    }
}

macro_rules! async_read_test {
    ($($input:expr, $expected:expr);+ $(;)?) => {
        let mut buf = [0u8; 30];
        $(
            let read = assert_ok!(Utf8Reader::new(MaybePending::new($input)).read(&mut buf).await);
            assert_eq!(&buf[..read], $expected);
        )+
    };
}

macro_rules! async_buf_read_test {
    ($($input:expr, $expected:expr);+ $(;)?) => {
        $(
            task::spawn(Utf8Reader::new(MaybePending::new($input))).enter(|cx, mut me| {
                assert_pending!(me.as_mut().poll_fill_buf(cx));
                let slice = assert_ready_ok!(me.poll_fill_buf(cx));
                assert_eq!(slice, $expected);
            });
        )+
    };
}

macro_rules! run_tests {
    ($($input:expr, $expected:expr);+ $(;)?) => {
        async_read_test!($($input, $expected);+);
        async_buf_read_test!($($input, $expected);+);
    };
}

#[tokio::test]
async fn parser_works() {
    run_tests! {
        EMPTY, EMPTY;

        ONE_BYTE_UTF8, ONE_BYTE_UTF8;
        &ONE_BYTE_UTF8[1..], &ONE_BYTE_UTF8[1..];

        TWO_BYTE_UTF8, TWO_BYTE_UTF8;
        &TWO_BYTE_UTF8[1..], &TWO_BYTE_UTF8[1..];
        &TWO_BYTE_UTF8[2..], &TWO_BYTE_UTF8[2..];
        &TWO_BYTE_UTF8[..(TWO_BYTE_UTF8.len() - 1)], &TWO_BYTE_UTF8[..(TWO_BYTE_UTF8.len() - 2)];
        &TWO_BYTE_UTF8[..(TWO_BYTE_UTF8.len() - 2)], &TWO_BYTE_UTF8[..(TWO_BYTE_UTF8.len() - 2)];

        THREE_BYTE_UTF8, THREE_BYTE_UTF8;
        &THREE_BYTE_UTF8[1..], &THREE_BYTE_UTF8[1..];
        &THREE_BYTE_UTF8[2..], &THREE_BYTE_UTF8[2..];
        &THREE_BYTE_UTF8[3..], &THREE_BYTE_UTF8[3..];
        &THREE_BYTE_UTF8[..(THREE_BYTE_UTF8.len() - 1)], &THREE_BYTE_UTF8[..(THREE_BYTE_UTF8.len() - 3)];
        &THREE_BYTE_UTF8[..(THREE_BYTE_UTF8.len() - 2)], &THREE_BYTE_UTF8[..(THREE_BYTE_UTF8.len() - 3)];
        &THREE_BYTE_UTF8[..(THREE_BYTE_UTF8.len() - 3)], &THREE_BYTE_UTF8[..(THREE_BYTE_UTF8.len() - 3)];

        FOUR_BYTE_UTF8, FOUR_BYTE_UTF8;
        &FOUR_BYTE_UTF8[1..], &FOUR_BYTE_UTF8[1..];
        &FOUR_BYTE_UTF8[2..], &FOUR_BYTE_UTF8[2..];
        &FOUR_BYTE_UTF8[3..], &FOUR_BYTE_UTF8[3..];
        &FOUR_BYTE_UTF8[4..], &FOUR_BYTE_UTF8[4..];
        &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 1)], &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 4)];
        &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 2)], &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 4)];
        &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 3)], &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 4)];
        &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 4)], &FOUR_BYTE_UTF8[..(FOUR_BYTE_UTF8.len() - 4)];
    }
}

#[tokio::test]
async fn middle_is_invalid() {
    let mut middle_invalid = [0u8; FOUR_BYTE_UTF8.len() - 2];
    middle_invalid[..9].copy_from_slice(&FOUR_BYTE_UTF8[..9]);
    middle_invalid[9..].copy_from_slice(&FOUR_BYTE_UTF8[11..]);
    run_tests! {
        &middle_invalid[..], &middle_invalid[..];
        &middle_invalid[..(middle_invalid.len() - 2)], &middle_invalid[..(middle_invalid.len() - 4)];
    }
}

struct Splitter<'a, I> {
    inner: &'a [u8],
    segments: I,
    current_range: (usize, usize),
}

impl<'a, I: Iterator<Item = usize> + Unpin> Splitter<'a, I> {
    fn new(inner: &'a [u8], segments: impl IntoIterator<IntoIter = I>) -> Self {
        let mut segments = segments.into_iter();
        let current_range = (0, segments.next().unwrap_or(inner.len()));
        Self {
            inner,
            segments,
            current_range,
        }
    }
}

impl<'a, I: Iterator<Item = usize> + Unpin> AsyncRead for Splitter<'a, I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let (lower, upper) = self.current_range;
        ready!(Pin::new(&mut &self.inner[lower..upper]).poll_read(cx, buf))?;
        self.current_range = (upper, self.segments.next().unwrap_or(self.inner.len()));
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn split_utf8() {
    let reader = Utf8Reader::new(Splitter::new(FOUR_BYTE_UTF8, vec![6, 13, 16]));
    task::spawn(reader).enter(|cx, mut me| {
        let mut buf = [0; 30];
        let mut buf = ReadBuf::new(&mut buf);

        assert_ready_ok!(me.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), &FOUR_BYTE_UTF8[..4]);

        assert_ready_ok!(me.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), &FOUR_BYTE_UTF8[..12]);

        assert_ready_ok!(me.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), &FOUR_BYTE_UTF8[..16]);

        assert_ready_ok!(me.poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), FOUR_BYTE_UTF8);
    })
}

#[tokio::test]
async fn only_partial_char() {
    let reader = Utf8Reader::new(&FOUR_BYTE_UTF8[..3]);
    task::spawn(reader).enter(|cx, me| {
        assert_ne!(assert_ready_ok!(me.poll_fill_buf(cx)), b"");
    });
}

#[tokio::test]
async fn too_short_read_buf() {
    let mut buf = [0; 3];
    let mut buf = ReadBuf::new(&mut buf);

    let reader = Utf8Reader::new(MaybePending::new(FOUR_BYTE_UTF8));

    let mut task = task::spawn(reader);
    task.enter(|cx, mut reader| {
        assert_pending!(reader.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled().len(), 0);
        assert_ready_ok!(reader.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled().len(), 3);
        assert_eq!(buf.filled(), &FOUR_BYTE_UTF8[..3]);
    });
}
