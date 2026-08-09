#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::AsyncReadExt;
use tokio_test::assert_ok;

#[tokio::test]
async fn chain() {
    let mut buf = Vec::new();
    let rd1: &[u8] = b"hello ";
    let rd2: &[u8] = b"world";

    let mut rd = rd1.chain(rd2);
    assert_ok!(rd.read_to_end(&mut buf).await);
    assert_eq!(buf, b"hello world");
}

#[tokio::test]
async fn empty_read_does_not_skip_first() {
    let mut buf = Vec::new();
    let rd1: &[u8] = b"hello ";
    let rd2: &[u8] = b"world";

    let mut rd = rd1.chain(rd2);
    assert_eq!(assert_ok!(rd.read(&mut []).await), 0);
    assert_ok!(rd.read_to_end(&mut buf).await);
    assert_eq!(buf, b"hello world");
}
