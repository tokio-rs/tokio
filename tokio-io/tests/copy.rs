#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio_io::{AsyncRead, AsyncReadExt};
use tokio_test::assert_ok;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn copy() {
    struct Rd(bool);

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            if self.0 {
                buf[0..11].copy_from_slice(b"hello world");
                self.0 = false;
                Poll::Ready(Ok(11))
            } else {
                Poll::Ready(Ok(0))
            }
        }
    }

    let mut rd = Rd(true);
    let mut wr = Vec::new();

    let n = assert_ok!(rd.copy(&mut wr).await);
    assert_eq!(n, 11);
    assert_eq!(wr, b"hello world");
}
