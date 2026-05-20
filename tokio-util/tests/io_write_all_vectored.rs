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

#[tokio::test]
async fn write_all_vectored_partial_writes() {
    // Writer that reports writing only `limit` bytes per call, exercising the
    // partial write path inside `write_all_vectored`. This is the path that
    // previously dropped or duplicated bytes when `advance_slices` advanced the
    // first remaining buffer incorrectly.
    struct LimitedWr {
        buf: BytesMut,
        limit: usize,
    }
    impl AsyncWrite for LimitedWr {
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
            let mut written = 0;
            for buf in bufs {
                let take = std::cmp::min(self.limit - written, buf.len());
                self.buf.extend_from_slice(&buf[..take]);
                written += take;
                if written == self.limit {
                    break;
                }
            }
            Ok(written).into()
        }
        fn is_write_vectored(&self) -> bool {
            true
        }
    }

    // case 1: write count lands exactly on a buffer boundary.
    // Previously, `advance_slices` would replace the next buffer with an
    // empty subslice, causing the data to be silently dropped.
    let mut wr = LimitedWr {
        buf: BytesMut::with_capacity(64),
        limit: 2,
    };
    let buf = &mut [IoSlice::new(b"ab"), IoSlice::new(b"cd")];
    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"abcd");

    // case 2: partial write inside a buffer.
    // Previously, `advance_slices` would resume from the already-written
    // prefix (e.g. "hel" instead of "lo"), duplicating bytes and skipping
    // the unwritten suffix.
    let mut wr = LimitedWr {
        buf: BytesMut::with_capacity(64),
        limit: 3,
    };
    let buf = &mut [IoSlice::new(b"hello"), IoSlice::new(b"world")];
    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"helloworld");

    // case 3: partial write that spans into the second buffer.
    let mut wr = LimitedWr {
        buf: BytesMut::with_capacity(64),
        limit: 4,
    };
    let buf = &mut [
        IoSlice::new(b"ab"),
        IoSlice::new(b"cdef"),
        IoSlice::new(b"gh"),
    ];
    write_all_vectored(&mut wr, buf).await.unwrap();
    assert_eq!(&wr.buf[..], b"abcdefgh");
}
