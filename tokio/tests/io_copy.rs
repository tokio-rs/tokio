#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use bytes::BytesMut;
use tokio::io::{self, AsyncRead, ReadBuf, AsyncWrite, AsyncWriteExt};
use tokio_test::assert_ok;
use futures::ready;

use std::pin::Pin;
use std::task::{Context, Poll};

#[tokio::test]
async fn copy() {
    struct Rd(bool);

    impl AsyncRead for Rd {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.0 {
                buf.put_slice(b"hello world");
                self.0 = false;
                Poll::Ready(Ok(()))
            } else {
                Poll::Ready(Ok(()))
            }
        }
    }

    let mut rd = Rd(true);
    let mut wr = Vec::new();

    let n = assert_ok!(io::copy(&mut rd, &mut wr).await);
    assert_eq!(n, 11);
    assert_eq!(wr, b"hello world");
}

#[tokio::test]
async fn proxy() {
    struct LimitRd {
        count: usize,
        reader: io::DuplexStream
    }

    struct BufferedWd {
        buf: BytesMut,
        writer: io::DuplexStream
    }

    impl AsyncRead for LimitRd {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();

            if this.count < 1024 {
                let read_before = buf.filled().len();
                ready!(Pin::new(&mut this.reader).poll_read(cx, buf))?;
                let read_after = buf.filled().len();
                this.count += read_after - read_before;
            }

            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for BufferedWd {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.get_mut().buf.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();

            while !this.buf.is_empty() {
                let n = ready!(Pin::new(&mut this.writer).poll_write(cx, &this.buf))?;
                let _ = this.buf.split_to(n);
            }

            Pin::new(&mut this.writer).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.writer).poll_shutdown(cx)
        }
    }

    let (rd, wd) = io::duplex(1024);
    let mut rd = LimitRd {
        count: 0,
        reader: rd
    };
    let mut wd = BufferedWd {
        buf: BytesMut::new(),
        writer: wd
    };

    // write start bytes
    assert_ok!(wd.write_all(&[0x42; 512]).await);
    assert_ok!(wd.flush().await);

    let n = assert_ok!(io::copy(&mut rd, &mut wd).await);

    assert_eq!(n, 1024);
}
