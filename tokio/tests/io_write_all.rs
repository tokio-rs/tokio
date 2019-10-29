#![warn(rust_2018_idioms)]

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_test::assert_ok;

use bytes::BytesMut;
use std::cmp;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn write_all() {
    struct Wr {
        buf: BytesMut,
        cnt: usize,
    }

    impl AsyncWrite for Wr {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let n = cmp::min(4, buf.len());
            let buf = &buf[0..n];

            self.cnt += 1;
            self.buf.extend(buf);
            Ok(buf.len()).into()
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
    }

    let mut wr = Wr {
        buf: BytesMut::with_capacity(64),
        cnt: 0,
    };

    assert_ok!(wr.write_all(b"hello world").await);
    assert_eq!(wr.buf, b"hello world"[..]);
    assert_eq!(wr.cnt, 3);
}
