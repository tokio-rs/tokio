#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::AsyncReadExt;
use tokio_test::assert_ok;

#[tokio::test]
async fn read_to_end() {
    let mut buf = vec![];
    let mut rd: &[u8] = b"hello world";

    let n = assert_ok!(rd.read_to_end(&mut buf).await);
    assert_eq!(n, 11);
    assert_eq!(buf[..], b"hello world"[..]);
}
