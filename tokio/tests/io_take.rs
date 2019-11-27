#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::AsyncReadExt;
use tokio_test::assert_ok;

#[tokio::test]
async fn take() {
    let mut buf = [0; 6];
    let rd: &[u8] = b"hello world";

    let mut rd = rd.take(4);
    let n = assert_ok!(rd.read(&mut buf).await);
    assert_eq!(n, 4);
    assert_eq!(&buf, &b"hell\0\0"[..]);
}
