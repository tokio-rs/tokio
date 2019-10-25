use tokio::io::AsyncRead;
use tokio_test::task::MockTask;
use tokio_test::{assert_ready_err, assert_ready_ok};

use bytes::{BufMut, BytesMut};
use futures_util::pin_mut;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[test]
fn assert_obj_safe() {
    fn _assert<T>() {}
    _assert::<Box<dyn AsyncRead>>();
}

#[test]
fn read_buf_success() {
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

    let mut buf = BytesMut::with_capacity(65);
    let mut task = MockTask::new();

    task.enter(|cx| {
        let rd = Rd;
        pin_mut!(rd);

        let n = assert_ready_ok!(rd.poll_read_buf(cx, &mut buf));

        assert_eq!(11, n);
        assert_eq!(buf[..], b"hello world"[..]);
    });
}

#[test]
fn read_buf_error() {
    struct Rd;

    impl AsyncRead for Rd {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let err = io::ErrorKind::Other.into();
            Poll::Ready(Err(err))
        }
    }

    let mut buf = BytesMut::with_capacity(65);
    let mut task = MockTask::new();

    task.enter(|cx| {
        let rd = Rd;
        pin_mut!(rd);

        let err = assert_ready_err!(rd.poll_read_buf(cx, &mut buf));
        assert_eq!(err.kind(), io::ErrorKind::Other);
    });
}

#[test]
fn read_buf_no_capacity() {
    struct Rd;

    impl AsyncRead for Rd {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            unimplemented!();
        }
    }

    // Can't create BytesMut w/ zero capacity, so fill it up
    let mut buf = BytesMut::with_capacity(64);
    let mut task = MockTask::new();

    buf.put(&[0; 64][..]);

    task.enter(|cx| {
        let rd = Rd;
        pin_mut!(rd);

        let n = assert_ready_ok!(rd.poll_read_buf(cx, &mut buf));
        assert_eq!(0, n);
    });
}

#[test]
fn read_buf_no_uninitialized() {
    struct Rd;

    impl AsyncRead for Rd {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            for b in buf {
                assert_eq!(0, *b);
            }

            Poll::Ready(Ok(0))
        }
    }

    let mut buf = BytesMut::with_capacity(64);
    let mut task = MockTask::new();

    task.enter(|cx| {
        let rd = Rd;
        pin_mut!(rd);

        let n = assert_ready_ok!(rd.poll_read_buf(cx, &mut buf));
        assert_eq!(0, n);
    });
}

#[test]
fn read_buf_uninitialized_ok() {
    struct Rd;

    impl AsyncRead for Rd {
        unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
            false
        }

        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            assert_eq!(buf[0..11], b"hello world"[..]);
            Poll::Ready(Ok(0))
        }
    }

    // Can't create BytesMut w/ zero capacity, so fill it up
    let mut buf = BytesMut::with_capacity(64);
    let mut task = MockTask::new();

    unsafe {
        buf.bytes_mut()[0..11].copy_from_slice(b"hello world");
    }

    task.enter(|cx| {
        let rd = Rd;
        pin_mut!(rd);

        let n = assert_ready_ok!(rd.poll_read_buf(cx, &mut buf));
        assert_eq!(0, n);
    });
}
