#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_test::assert_ok;

use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};

#[tokio::test]
async fn read_to_end() {
    struct Rd {
        val: &'static [u8],
    }

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let me = &mut *self;
            let len = cmp::min(buf.len(), me.val.len());

            buf[..len].copy_from_slice(&me.val[..len]);
            me.val = &me.val[len..];
            Poll::Ready(Ok(len))
        }
    }

    let mut buf = vec![];
    let mut rd = Rd {
        val: b"hello world",
    };

    let n = assert_ok!(rd.read_to_end(&mut buf).await);
    assert_eq!(n, 11);
    assert_eq!(buf[..], b"hello world"[..]);
}
