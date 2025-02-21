#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io;

use tokio::io::AsyncReadExt;
use tokio_test::assert_ok;

#[tokio::test]
async fn read_buf_exact() {
    let mut buf = [0u8; 8];
    let mut rd: &[u8] = b"hello world";

    let n = assert_ok!(rd.read_buf_exact(&mut buf.as_mut_slice()).await);
    assert_eq!(n, 8);
    assert_eq!(buf[..], b"hello wo"[..]);
}

#[tokio::test]
async fn read_buf_exact_eof() {
    let mut buf = [0u8; 12];
    let mut rd: &[u8] = b"hello world";

    let error = rd
        .read_buf_exact(&mut buf.as_mut_slice())
        .await
        .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}
