#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_test::assert_ok;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn read_exact() {
    struct Rd {
        val: &'static [u8; 11],
    }

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let me = &mut *self;
            let len = buf.len();

            buf[..].copy_from_slice(&me.val[..len]);
            Poll::Ready(Ok(buf.len()))
        }
    }

    let mut buf = Box::new([0; 8]);
    let mut rd = Rd {
        val: b"hello world",
    };

    let n = assert_ok!(rd.read_exact(&mut buf[..]).await);
    assert_eq!(n, 8);
    assert_eq!(buf[..], b"hello wo"[..]);
}
