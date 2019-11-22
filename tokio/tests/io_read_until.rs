#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::AsyncBufReadExt;
use tokio_test::assert_ok;

#[tokio::test]
async fn read_until() {
    let mut buf = vec![];
    let mut rd: &[u8] = b"hello world";

    let n = assert_ok!(rd.read_until(b' ', &mut buf).await);
    assert_eq!(n, 6);
    assert_eq!(buf, b"hello ");
    buf.clear();
    let n = assert_ok!(rd.read_until(b' ', &mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf, b"world");
    buf.clear();
    let n = assert_ok!(rd.read_until(b' ', &mut buf).await);
    assert_eq!(n, 0);
    assert_eq!(buf, []);
}
