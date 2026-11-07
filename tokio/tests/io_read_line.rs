#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "io-util"
    )
))]

use std::io::ErrorKind;
use std::str::Utf8Error;
use tokio::io::{AsyncBufReadExt, BufReader, Error};
use tokio_test::{assert_ok, io::Builder};

use std::io::Cursor;

#[tokio::test]
async fn read_line() {
    let mut buf = String::new();
    let mut rd = Cursor::new(b"hello\nworld\n\n");

    let n = assert_ok!(rd.read_line(&mut buf).await);
    assert_eq!(n, 6);
    assert_eq!(buf, "hello\n");
    buf.clear();
    let n = assert_ok!(rd.read_line(&mut buf).await);
    assert_eq!(n, 6);
    assert_eq!(buf, "world\n");
    buf.clear();
    let n = assert_ok!(rd.read_line(&mut buf).await);
    assert_eq!(n, 1);
    assert_eq!(buf, "\n");
    buf.clear();
    let n = assert_ok!(rd.read_line(&mut buf).await);
    assert_eq!(n, 0);
    assert_eq!(buf, "");
}

#[tokio::test]
async fn read_line_not_all_ready() {
    let mock = Builder::new()
        .read(b"Hello Wor")
        .read(b"ld\nFizzBuz")
        .read(b"z\n1\n2")
        .build();

    let mut read = BufReader::new(mock);

    let mut line = "We say ".to_string();
    let bytes = read.read_line(&mut line).await.unwrap();
    assert_eq!(bytes, "Hello World\n".len());
    assert_eq!(line.as_str(), "We say Hello World\n");

    line = "I solve ".to_string();
    let bytes = read.read_line(&mut line).await.unwrap();
    assert_eq!(bytes, "FizzBuzz\n".len());
    assert_eq!(line.as_str(), "I solve FizzBuzz\n");

    line.clear();
    let bytes = read.read_line(&mut line).await.unwrap();
    assert_eq!(bytes, 2);
    assert_eq!(line.as_str(), "1\n");

    line.clear();
    let bytes = read.read_line(&mut line).await.unwrap();
    assert_eq!(bytes, 1);
    assert_eq!(line.as_str(), "2");
}

#[tokio::test]
async fn read_line_invalid_utf8() {
    let mock = Builder::new().read(b"Hello Wor\xffld.\n").build();

    let mut read = BufReader::new(mock);

    let mut line = "Foo".to_string();
    let err = read.read_line(&mut line).await.expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    let utf8 = err.into_inner().unwrap().downcast::<Utf8Error>().unwrap();
    assert_eq!(utf8.valid_up_to(), 12);
    assert_eq!(line.as_str(), "Foo");
}

#[tokio::test]
async fn read_line_fail() {
    let mock = Builder::new()
        .read(b"Hello Wor")
        .read_error(Error::other("The world has no end"))
        .build();

    let mut read = BufReader::new(mock);

    let mut line = "Foo".to_string();
    let err = read.read_line(&mut line).await.expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::Other);
    assert_eq!(err.to_string(), "The world has no end");
    assert_eq!(line.as_str(), "FooHello Wor");
}

#[tokio::test]
async fn read_line_fail_and_utf8_fail() {
    let mock = Builder::new()
        .read(b"Hello Wor")
        .read(b"\xff\xff\xff")
        .read_error(Error::other("The world has no end"))
        .build();

    let mut read = BufReader::new(mock);

    let mut line = "Foo".to_string();
    let err = read.read_line(&mut line).await.expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::Other);
    assert_eq!(err.to_string(), "The world has no end");
    assert_eq!(line.as_str(), "Foo");
}

#[tokio::test]
async fn read_line_appends_char_split_across_reads() {
    let mock = Builder::new().read(b"caf\xc3").read(b"\xa9\n").build();

    let mut read = BufReader::new(mock);

    let mut line = "le ".to_string();
    let bytes = read.read_line(&mut line).await.unwrap();
    assert_eq!(bytes, 6);
    assert_eq!(line.as_str(), "le café\n");
}

#[tokio::test]
async fn read_line_invalid_utf8_after_multibyte_char() {
    // The line starts with a continuation byte, which must not be accepted
    // just because the existing string ends with a complete character.
    let mock = Builder::new().read(b"\xa9\n").build();

    let mut read = BufReader::new(mock);

    let mut line = "café".to_string();
    let err = read.read_line(&mut line).await.expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    let utf8 = err.into_inner().unwrap().downcast::<Utf8Error>().unwrap();
    assert_eq!(utf8.valid_up_to(), 5);
    assert_eq!(line.as_str(), "café");
}

#[test]
fn read_line_panic_in_consume() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

    struct PanickingReader(bool);

    impl AsyncRead for PanickingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            unreachable!()
        }
    }

    impl AsyncBufRead for PanickingReader {
        fn poll_fill_buf(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<&[u8]>> {
            if self.0 {
                Poll::Ready(Ok(b"\xff"))
            } else {
                Poll::Ready(Ok(b"\n"))
            }
        }

        fn consume(mut self: Pin<&mut Self>, _amt: usize) {
            if self.0 {
                self.0 = false;
                panic!("boom");
            }
        }
    }

    let mut rd = PanickingReader(true);
    let mut line = String::from("abc");
    let mut fut = Box::pin(rd.read_line(&mut line));

    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = fut.as_mut().poll(&mut cx);
    }));

    let res = fut.as_mut().poll(&mut cx);
    for _ in line.chars() {}
    assert!(matches!(res, Poll::Ready(Err(ref e)) if e.kind() == ErrorKind::InvalidData));
    assert_eq!(line, "abc");
}
