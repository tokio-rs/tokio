#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use futures_util::StreamExt;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead};
use tokio_test::assert_ok;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn lines() {
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

    let rd = Rd {
        val: b"hello\r\nworld\n\n",
    };
    let mut st = rd.lines();

    let b = assert_ok!(st.next().await.unwrap());
    assert_eq!(b, "hello");
    let b = assert_ok!(st.next().await.unwrap());
    assert_eq!(b, "world");
    let b = assert_ok!(st.next().await.unwrap());
    assert_eq!(b, "");
    assert!(st.next().await.is_none());
}
