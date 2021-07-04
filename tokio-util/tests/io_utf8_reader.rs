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
const ENGLISH: &[u8] = b"test string";
// "test string" Google-translated to Arabic
// each Unicode character is 2 bytes in width
const ARABIC: &[u8] = "سلسلة الاختبار".as_bytes();
// "test string" Google-translated to Chinese
// each Unicode character is 3 bytes in width
const CHINESE: &[u8] = "测试字符串".as_bytes();
// each Unicode character is 4 bytes in width
const EMOJIS: &[u8] = "😀😬😁😂😃".as_bytes();

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
async fn empty() {
    run_tests!(EMPTY, EMPTY);
}

#[tokio::test]
async fn english() {
    run_tests! {
        ENGLISH, ENGLISH;
        &ENGLISH[1..], &ENGLISH[1..];
    };
}

#[tokio::test]
async fn arabic() {
    run_tests! {
        ARABIC, ARABIC;
        &ARABIC[1..], &ARABIC[1..];
        &ARABIC[2..], &ARABIC[2..];
        &ARABIC[..(ARABIC.len() - 1)], &ARABIC[..(ARABIC.len() - 2)];
        &ARABIC[..(ARABIC.len() - 2)], &ARABIC[..(ARABIC.len() - 2)];
    }
}

#[tokio::test]
async fn chinese() {
    run_tests! {
        CHINESE, CHINESE;
        &CHINESE[1..], &CHINESE[1..];
        &CHINESE[2..], &CHINESE[2..];
        &CHINESE[3..], &CHINESE[3..];
        &CHINESE[..(CHINESE.len() - 1)], &CHINESE[..(CHINESE.len() - 3)];
        &CHINESE[..(CHINESE.len() - 2)], &CHINESE[..(CHINESE.len() - 3)];
        &CHINESE[..(CHINESE.len() - 3)], &CHINESE[..(CHINESE.len() - 3)];
    }
}

#[tokio::test]
async fn emojis() {
    run_tests! {
        EMOJIS, EMOJIS;
        &EMOJIS[1..], &EMOJIS[1..];
        &EMOJIS[2..], &EMOJIS[2..];
        &EMOJIS[3..], &EMOJIS[3..];
        &EMOJIS[4..], &EMOJIS[4..];
        &EMOJIS[..(EMOJIS.len() - 1)], &EMOJIS[..(EMOJIS.len() - 4)];
        &EMOJIS[..(EMOJIS.len() - 2)], &EMOJIS[..(EMOJIS.len() - 4)];
        &EMOJIS[..(EMOJIS.len() - 3)], &EMOJIS[..(EMOJIS.len() - 4)];
        &EMOJIS[..(EMOJIS.len() - 4)], &EMOJIS[..(EMOJIS.len() - 4)];
    }
}

#[tokio::test]
async fn middle_is_invalid() {
    let mut middle_invalid = [0u8; EMOJIS.len() - 2];
    middle_invalid[..9].copy_from_slice(&EMOJIS[..9]);
    middle_invalid[9..].copy_from_slice(&EMOJIS[11..]);
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
    let reader = Utf8Reader::new(Splitter::new(EMOJIS, [6, 13, 16]));
    task::spawn(reader).enter(|cx, mut me| {
        let mut buf = [0; 30];
        let mut buf = ReadBuf::new(&mut buf);

        assert_ready_ok!(me.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), &EMOJIS[..4]);

        assert_ready_ok!(me.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), &EMOJIS[..12]);

        assert_ready_ok!(me.as_mut().poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), &EMOJIS[..16]);

        assert_ready_ok!(me.poll_read(cx, &mut buf));
        assert_eq!(buf.filled(), EMOJIS);
    })
}
