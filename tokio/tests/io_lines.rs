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

use std::io::{Error, ErrorKind};
use std::string::FromUtf8Error;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::timeout;
use tokio_test::assert_ok;

#[tokio::test]
async fn lines_inherent() {
    let rd: &[u8] = b"hello\r\nworld\n\n";
    let mut st = rd.lines();

    let b = assert_ok!(st.next_line().await).unwrap();
    assert_eq!(b, "hello");
    let b = assert_ok!(st.next_line().await).unwrap();
    assert_eq!(b, "world");
    let b = assert_ok!(st.next_line().await).unwrap();
    assert_eq!(b, "");
    assert!(assert_ok!(st.next_line().await).is_none());
}

#[tokio::test]
async fn lines_keeps_partial_line_after_io_error() {
    let mock = tokio_test::io::Builder::new()
        .read(b"abc")
        .read_error(Error::new(ErrorKind::Other, "boom"))
        .read(b"def\nghi\n")
        .build();
    let mut lines = BufReader::new(mock).lines();

    let err = lines.next_line().await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Other);

    assert_eq!(lines.next_line().await.unwrap(), Some("abcdef".to_string()));
    assert_eq!(lines.next_line().await.unwrap(), Some("ghi".to_string()));
    assert_eq!(lines.next_line().await.unwrap(), None);
}

#[tokio::test]
async fn lines_keeps_truncated_multibyte_char_after_io_error() {
    let mock = tokio_test::io::Builder::new()
        .read(b"ab\xc3")
        .read_error(Error::new(ErrorKind::Other, "boom"))
        .read(b"\xa9cd\nghi\n")
        .build();
    let mut lines = BufReader::new(mock).lines();

    let err = lines.next_line().await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Other);

    assert_eq!(lines.next_line().await.unwrap(), Some("abécd".to_string()));
    assert_eq!(lines.next_line().await.unwrap(), Some("ghi".to_string()));
    assert_eq!(lines.next_line().await.unwrap(), None);
}

#[tokio::test]
async fn lines_invalid_utf8_line_errors_once_and_advances() {
    let rd: &[u8] = b"ok\n\xff\xfe\nnext\n";
    let mut lines = rd.lines();

    assert_eq!(lines.next_line().await.unwrap(), Some("ok".to_string()));

    let err = lines.next_line().await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);

    let inner = err.into_inner().unwrap();
    let utf8 = inner.downcast::<FromUtf8Error>().unwrap();
    assert_eq!(utf8.into_bytes(), b"\xff\xfe");

    assert_eq!(lines.next_line().await.unwrap(), Some("next".to_string()));
    assert_eq!(lines.next_line().await.unwrap(), None);
}

#[tokio::test]
async fn lines_invalid_utf8_at_eof_does_not_loop_forever() {
    let rd: &[u8] = b"ok\n\xff";
    let mut lines = rd.lines();

    assert_eq!(lines.next_line().await.unwrap(), Some("ok".to_string()));

    let err = lines.next_line().await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);

    assert_eq!(lines.next_line().await.unwrap(), None);
}

#[tokio::test(start_paused = true)]
async fn lines_next_line_is_cancel_safe() {
    let mock = tokio_test::io::Builder::new()
        .read(b"hello")
        .wait(Duration::from_secs(1))
        .read(b"\nworld\n")
        .build();
    let mut lines = BufReader::new(mock).lines();

    assert!(timeout(Duration::from_millis(1), lines.next_line())
        .await
        .is_err());

    assert_eq!(lines.next_line().await.unwrap(), Some("hello".to_string()));
    assert_eq!(lines.next_line().await.unwrap(), Some("world".to_string()));
}
