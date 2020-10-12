#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{self, AsyncRead, ReadBuf};
use tokio_test::assert_ok;

use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn copy() {
    struct Rd(bool);

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.0 {
                buf.put_slice(b"hello world");
                self.0 = false;
                Poll::Ready(Ok(()))
            } else {
                Poll::Ready(Ok(()))
            }
        }
    }

    let mut rd = Rd(true);
    let mut wr = Vec::new();

    let n = assert_ok!(io::copy(&mut rd, &mut wr).await);
    assert_eq!(n, 11);
    assert_eq!(wr, b"hello world");
}
