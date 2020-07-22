#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_test::assert_ok;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn read() {
    #[derive(Default)]
    struct Rd {
        poll_cnt: usize,
    }

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            assert_eq!(0, self.poll_cnt);
            self.poll_cnt += 1;

            buf[0..11].copy_from_slice(b"hello world");
            Poll::Ready(Ok(11))
        }
    }

    let mut buf = Box::new([0; 11]);
    let mut rd = Rd::default();

    let n = assert_ok!(rd.read(&mut buf[..]).await);
    assert_eq!(n, 11);
    assert_eq!(buf[..], b"hello world"[..]);
}

struct BadAsyncRead;

impl AsyncRead for BadAsyncRead {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        for b in &mut *buf {
            *b = b'a';
        }
        Poll::Ready(Ok(buf.len() * 2))
    }
}

#[tokio::test]
#[should_panic]
async fn read_buf_bad_async_read() {
    let mut buf = Vec::with_capacity(10);
    BadAsyncRead.read_buf(&mut buf).await.unwrap();
}
