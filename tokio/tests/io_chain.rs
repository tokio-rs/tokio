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

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_test::assert_ok;

#[tokio::test]
async fn chain() {
    let mut buf = Vec::new();
    let rd1: &[u8] = b"hello ";
    let rd2: &[u8] = b"world";

    let mut rd = rd1.chain(rd2);
    assert_ok!(rd.read_to_end(&mut buf).await);
    assert_eq!(buf, b"hello world");
}

#[tokio::test]
async fn empty_read_does_not_skip_first() {
    let mut buf = Vec::new();
    let rd1: &[u8] = b"hello ";
    let rd2: &[u8] = b"world";

    let mut rd = rd1.chain(rd2);
    assert_eq!(assert_ok!(rd.read(&mut []).await), 0);
    assert_ok!(rd.read_to_end(&mut buf).await);
    assert_eq!(buf, b"hello world");
}

#[tokio::test]
async fn empty_read_does_not_poll_inner() {
    struct PanicOnPoll;

    impl AsyncRead for PanicOnPoll {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            panic!("empty Chain reads must not poll the inner reader");
        }
    }

    let mut rd = PanicOnPoll.chain(&b"tail"[..]);
    assert_eq!(assert_ok!(rd.read(&mut []).await), 0);

    let mut storage = [0; 2];
    let mut buf = ReadBuf::new(&mut storage);
    buf.put_slice(b"hi");
    std::future::poll_fn(|cx| Pin::new(&mut rd).poll_read(cx, &mut buf))
        .await
        .unwrap();
    assert_eq!(buf.filled(), b"hi");
}
