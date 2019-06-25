use bytes::BytesMut;
use pin_utils::pin_mut;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_test::assert_ready_ok;
use tokio_test::task::MockTask;

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

        let write = tokio::io::write(&mut wr, "hello world".as_bytes());
        pin_mut!(write);

        let n = assert_ready_ok!(write.poll(cx));
        assert_eq!(n, 11);
    });
}

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

        let read = tokio::io::read(&mut rd, &mut buf[..]);
        pin_mut!(read);

        let n = assert_ready_ok!(read.poll(cx));
        assert_eq!(n, 11);
        assert_eq!(buf[..], b"hello world"[..]);
    });
}

#[test]
fn copy() {
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

    struct Wr(BytesMut);

    impl Unpin for Wr {}
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

    let buf = BytesMut::with_capacity(64);
    let mut task = MockTask::new();

    task.enter(|cx| {
        let mut rd = Rd(true);
        let mut wr = Wr(buf);

        let copy = tokio::io::copy(&mut rd, &mut wr);
        pin_mut!(copy);

        let n = assert_ready_ok!(copy.poll(cx));

        assert_eq!(n, 11);
        assert_eq!(wr.0[..], b"hello world"[..]);
    });
}
