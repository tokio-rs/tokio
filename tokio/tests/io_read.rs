#![deny(warnings, rust_2018_idioms)]

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_test::assert_ready_ok;
use tokio_test::task::MockTask;

use pin_utils::pin_mut;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[test]
fn read() {
    struct Rd;

    impl AsyncRead for Rd {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            buf[0..11].copy_from_slice(b"hello world");
            Poll::Ready(Ok(11))
        }
    }

    let mut buf = Box::new([0; 11]);
    let mut task = MockTask::new();

    task.enter(|cx| {
        let mut rd = Rd;

        let read = rd.read(&mut buf[..]);
        pin_mut!(read);

        let n = assert_ready_ok!(read.poll(cx));
        assert_eq!(n, 11);
        assert_eq!(buf[..], b"hello world"[..]);
    });
}
