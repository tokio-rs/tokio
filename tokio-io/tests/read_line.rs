#![warn(rust_2018_idioms)]

use std::io::Cursor;
use tokio_io::AsyncBufReadExt;
use tokio_test::assert_ok;

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
