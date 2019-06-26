#![deny(warnings, rust_2018_idioms)]

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_test::assert_ready_ok;
use tokio_test::task::MockTask;

use bytes::BytesMut;
use pin_utils::pin_mut;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[test]
fn write() {
    struct Wr(BytesMut);

    impl AsyncWrite for Wr {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.0.extend(buf);
            Ok(buf.len()).into()
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
    }

    let mut task = MockTask::new();

    task.enter(|cx| {
        let mut wr = Wr(BytesMut::with_capacity(64));

        let write = wr.write(b"hello world");
        pin_mut!(write);

        let n = assert_ready_ok!(write.poll(cx));
        assert_eq!(n, 11);
    });
}
