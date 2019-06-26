#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_test::assert_ok;

use bytes::BytesMut;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn write() {
    struct Wr(BytesMut);

    impl AsyncWrite for Wr {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.0.extend(buf);
            Ok(buf.len()).into()
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
    }

    let mut wr = Wr(BytesMut::with_capacity(64));

    let n = assert_ok!(wr.write(b"hello world").await);
    assert_eq!(n, 11);
}
