#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_test::assert_ok;

use futures::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn read_buf() {
    struct Rd {
        cnt: usize,
    }

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.cnt += 1;
            buf.put_slice(b"hello world");
            Poll::Ready(Ok(()))
        }
    }

    let mut buf = vec![];
    let mut rd = Rd { cnt: 0 };

    let n = assert_ok!(rd.read_buf(&mut buf).await);
    assert_eq!(1, rd.cnt);
    assert_eq!(n, 11);
    assert_eq!(buf[..], b"hello world"[..]);
}

#[tokio::test]
async fn poll_read_buf() {
    struct Rd {
        cnt: usize,
    }

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.cnt += 1;
            buf.put_slice(b"hello world");
            Poll::Ready(Ok(()))
        }
    }

    let mut buf = vec![];
    let mut rd = Rd { cnt: 0 };

    let res = tokio::spawn(async move {
        poll_fn(|cx| {
            let res = rd.poll_read_buf(&mut buf, cx);
            assert_eq!(1, rd.cnt);
            assert_eq!(buf[..], b"hello world"[..]);
            res
        })
        .await
    })
    .await;

    assert!(matches!(res, Ok(Ok(11usize))));
}
