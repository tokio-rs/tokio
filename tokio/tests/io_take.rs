#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncRead, AsyncReadExt, ReadBuf};
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

struct BadReader;

impl AsyncRead for BadReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        read_buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let vec = vec![0; 10];

        let mut buf = ReadBuf::new(vec.leak());
        buf.put_slice(&[123; 10]);
        *read_buf = buf;

        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
#[should_panic]
async fn bad_reader_fails() {
    let mut buf = Vec::with_capacity(10);

    BadReader.take(10).read_buf(&mut buf).await.unwrap();
}
