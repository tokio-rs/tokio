#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncRead, ReadBuf};
use tokio_test::task;
use tokio_test::{assert_ready_err, assert_ready_ok};

use bytes::BytesMut;
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
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            buf.append(b"hello world");
            Poll::Ready(Ok(()))
        }
    }

    let mut buf = BytesMut::with_capacity(65);

    task::spawn(Rd).enter(|cx, rd| {
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
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let err = io::ErrorKind::Other.into();
            Poll::Ready(Err(err))
        }
    }

    let mut buf = BytesMut::with_capacity(65);

    task::spawn(Rd).enter(|cx, rd| {
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
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            unimplemented!();
        }
    }

    let mut buf = [0u8; 0];

    task::spawn(Rd).enter(|cx, rd| {
        let n = assert_ready_ok!(rd.poll_read_buf(cx, &mut &mut buf[..]));
        assert_eq!(0, n);
    });
}

#[test]
fn read_buf_uninitialized_ok() {
    struct Rd;

    impl AsyncRead for Rd {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            assert_eq!(buf.remaining(), 64);
            assert_eq!(buf.filled().len(), 0);
            assert_eq!(buf.initialized().len(), 0);
            Poll::Ready(Ok(()))
        }
    }

    // Can't create BytesMut w/ zero capacity, so fill it up
    let mut buf = BytesMut::with_capacity(64);

    task::spawn(Rd).enter(|cx, rd| {
        let n = assert_ready_ok!(rd.poll_read_buf(cx, &mut buf));
        assert_eq!(0, n);
    });
}
