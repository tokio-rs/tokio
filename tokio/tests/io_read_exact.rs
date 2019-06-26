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
fn read_exact() {
    struct Rd {
        val: &'static [u8; 11],
    }

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8]
        ) -> Poll<io::Result<usize>> {
            let me = &mut *self;
            let len = buf.len();

            buf[..].copy_from_slice(&me.val[..len]);
            Poll::Ready(Ok(buf.len()))
        }
    }

    let mut buf = Box::new([0; 8]);
    let mut task = MockTask::new();

    task.enter(|cx| {
        let mut rd = Rd { val: b"hello world" };

        let read = rd.read_exact(&mut buf[..]);
        pin_mut!(read);

        let n = assert_ready_ok!(read.poll(cx));
        assert_eq!(n, 8);
        assert_eq!(buf[..], b"hello wo"[..]);
    });
}
