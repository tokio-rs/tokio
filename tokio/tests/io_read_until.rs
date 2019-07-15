#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead};
use tokio_test::assert_ok;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn read_until() {
    struct Rd {
        val: &'static [u8],
    }

    impl AsyncRead for Rd {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            unimplemented!()
        }
    }

    impl AsyncBufRead for Rd {
        fn poll_fill_buf<'a>(
            self: Pin<&'a mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<io::Result<&'a [u8]>> {
            Poll::Ready(Ok(self.val))
        }

        fn consume(mut self: Pin<&mut Self>, amt: usize) {
            self.val = &self.val[amt..];
        }
    }

    let mut buf = vec![];
    let mut rd = Rd {
        val: b"hello world",
    };

    let n = assert_ok!(rd.read_until(b' ', &mut buf).await);
    assert_eq!(n, 6);
    assert_eq!(buf, b"hello ");
    buf.clear();
    let n = assert_ok!(rd.read_until(b' ', &mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf, b"world");
    buf.clear();
    let n = assert_ok!(rd.read_until(b' ', &mut buf).await);
    assert_eq!(n, 0);
    assert_eq!(buf, []);
}
