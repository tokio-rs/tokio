#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::AsyncWrite;
use tokio_util::io::write_all_vectored;

use bytes::BytesMut;
use std::io;
use std::io::IoSlice;
use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn test_write_all_vectored() {
    struct Wr {
        buf: BytesMut,
    }
    impl AsyncWrite for Wr {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            // When executing `write_all_buf` with this writer,
            // `poll_write` is not called.
            panic!("shouldn't be called")
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bufs: &[io::IoSlice<'_>],
        ) -> Poll<Result<usize, io::Error>> {
            for buf in bufs {
                self.buf.extend_from_slice(buf);
            }
            let n = self.buf.len();
            Ok(n).into()
        }
        fn is_write_vectored(&self) -> bool {
            // Enable vectored write. (doesn't need to be enabled explicitly for `write_all_vectored`)
            true
        }
    }

    let mut wr = Wr {
        buf: BytesMut::with_capacity(64),
    };

    let buf = &mut [
        IoSlice::new(&b"hello"[..]),
        IoSlice::new(&b" "[..]),
        IoSlice::new(&b"world"[..]),
    ];

    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"hello world");
}

#[tokio::test]
async fn write_all_vectored_with_empty_slice() {
    struct Wr {
        buf: BytesMut,
    }
    impl AsyncWrite for Wr {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            panic!("shouldn't be called")
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bufs: &[io::IoSlice<'_>],
        ) -> Poll<Result<usize, io::Error>> {
            for buf in bufs {
                self.buf.extend_from_slice(buf);
            }
            let n = self.buf.len();
            Ok(n).into()
        }
        fn is_write_vectored(&self) -> bool {
            // Enable vectored write.
            true
        }
    }

    // case 1 middle empty slice
    let mut wr = Wr {
        buf: BytesMut::with_capacity(64),
    };

    let buf = &mut [
        IoSlice::new(&b"hello"[..]),
        IoSlice::new(&[]),
        IoSlice::new(&b"world"[..]),
    ];

    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"helloworld");

    // case 2 no slices
    let mut wr = Wr {
        buf: BytesMut::with_capacity(64),
    };

    let buf = &mut [];

    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"");

    // case 3 just an empty slice
    let mut wr = Wr {
        buf: BytesMut::with_capacity(64),
    };
    let buf = &mut [IoSlice::new(&[])];

    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"");

    // case 4 ending with empty slice
    let mut wr = Wr {
        buf: BytesMut::with_capacity(64),
    };
    let buf = &mut [IoSlice::new(b"hello"), IoSlice::new(&[])];

    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"hello");
}
