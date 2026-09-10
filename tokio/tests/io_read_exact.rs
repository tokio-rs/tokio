#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "io-util"
    )
))]

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_test::assert_ok;

#[tokio::test]
async fn read_exact() {
    let mut buf = Box::new([0; 8]);
    let mut rd: &[u8] = b"hello world";

    let n = assert_ok!(rd.read_exact(&mut buf[..]).await);
    assert_eq!(n, 8);
    assert_eq!(buf[..], b"hello wo"[..]);
}

struct InterruptThenRead {
    interrupted: bool,
    data: &'static [u8],
}

impl AsyncRead for InterruptThenRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.interrupted {
            self.interrupted = true;
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::Interrupted)));
        }
        let n = std::cmp::min(self.data.len(), buf.remaining());
        buf.put_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn read_exact_retries_interrupted() {
    let mut reader = InterruptThenRead {
        interrupted: false,
        data: b"hello",
    };
    let mut buf = [0u8; 5];
    let n = reader.read_exact(&mut buf).await.unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf, b"hello");
}
