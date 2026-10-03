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

use bytes::BytesMut;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio_test::assert_ok;

use std::io::ErrorKind;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

mod support {
    pub mod io_coop;
}
use support::io_coop::{ByteAtATimeReader, ByteAtATimeWriter};

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

struct BufferedWd {
    buf: BytesMut,
    writer: io::DuplexStream,
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

#[tokio::test]
async fn proxy() {
    let (rd, wd) = io::duplex(1024);
    let mut rd = rd.take(1024);
    let mut wd = BufferedWd {
        buf: BytesMut::new(),
        writer: wd,
    };

    // write start bytes
    assert_ok!(wd.write_all(&[0x42; 512]).await);
    assert_ok!(wd.flush().await);

    let n = assert_ok!(io::copy(&mut rd, &mut wd).await);

    assert_eq!(n, 1024);
}

#[tokio::test]
async fn proxy_buf() {
    let (rd, wd) = io::duplex(1024);
    let mut rd = io::BufReader::new(rd).take(1024);
    let mut wd = BufferedWd {
        buf: BytesMut::new(),
        writer: wd,
    };

    // write start bytes
    assert_ok!(wd.write_all(&[0x42; 512]).await);
    assert_ok!(wd.flush().await);

    let n = assert_ok!(io::copy_buf(&mut rd, &mut wd).await);

    assert_eq!(n, 1024);
}

#[tokio::test]
async fn always_ready_reads_are_cooperative() {
    let expected = b"abcd".repeat(64);
    let mut reader = ByteAtATimeReader {
        data: &expected,
        interruptions_remaining: 0,
    };
    let mut output = Vec::new();
    let mut copy = tokio_test::task::spawn(io::copy(&mut reader, &mut output));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(output, expected);
}

#[tokio::test]
async fn always_ready_writes_are_cooperative() {
    let expected = b"abcd".repeat(64);
    let mut reader = &expected[..];
    let mut writer = ByteAtATimeWriter {
        data: Vec::new(),
        interruptions_remaining: 0,
    };
    let mut copy = tokio_test::task::spawn(io::copy(&mut reader, &mut writer));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(writer.data, expected);
}

#[tokio::test]
async fn interrupted_reads_remain_unconstrained() {
    let expected = b"abcd".repeat(64);
    let mut reader = ByteAtATimeReader {
        data: &expected,
        interruptions_remaining: 256,
    };
    let mut output = Vec::new();
    // disabling the budget lets the same input finish in a single poll
    let bytes_copied = {
        let copy = tokio::task::unconstrained(io::copy(&mut reader, &mut output));
        let mut copy = tokio_test::task::spawn(copy);
        tokio_test::assert_ready_ok!(copy.poll())
    };

    assert_eq!(bytes_copied, expected.len() as u64);
    assert_eq!(output, expected);
}

#[tokio::test]
async fn interrupted_writes_remain_unconstrained() {
    let expected = b"abcd".repeat(64);
    let mut reader = &expected[..];
    let mut writer = ByteAtATimeWriter {
        data: Vec::new(),
        interruptions_remaining: 256,
    };
    // disabling the budget lets Interrupted retries and writes finish in a single poll
    let bytes_copied = {
        let copy = tokio::task::unconstrained(io::copy(&mut reader, &mut writer));
        let mut copy = tokio_test::task::spawn(copy);
        tokio_test::assert_ready_ok!(copy.poll())
    };

    assert_eq!(bytes_copied, expected.len() as u64);
    assert_eq!(writer.data, expected);
}

#[tokio::test]
async fn retry_on_io_interrupted() {
    let mut reader = tokio_test::io::Builder::new()
        .read_error(ErrorKind::Interrupted.into())
        .read(b"ab")
        .read_error(ErrorKind::Interrupted.into())
        .read(b"cd")
        .build();
    let mut writer = tokio_test::io::Builder::new()
        .write_error(ErrorKind::Interrupted.into())
        .write(b"a")
        .write_error(ErrorKind::Interrupted.into())
        .write(b"bcd")
        .build();
    let count = tokio::io::copy(&mut reader, &mut writer).await;
    assert_eq!(count.unwrap(), 4);
}

#[tokio::test]
async fn interrupted_reads_are_cooperative() {
    let expected = b"abcd";
    let mut reader = ByteAtATimeReader {
        data: expected,
        interruptions_remaining: 256,
    };
    let mut output = Vec::new();
    let mut copy = tokio_test::task::spawn(io::copy(&mut reader, &mut output));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(output, expected);
}

#[tokio::test]
async fn interrupted_writes_are_cooperative() {
    let expected = b"abcd";
    let mut reader = &expected[..];
    let mut writer = ByteAtATimeWriter {
        data: Vec::new(),
        interruptions_remaining: 256,
    };
    let mut copy = tokio_test::task::spawn(io::copy(&mut reader, &mut writer));

    tokio_test::assert_pending!(copy.poll());
    assert_eq!(copy.await.unwrap(), expected.len() as u64);
    assert_eq!(writer.data, expected);
}
