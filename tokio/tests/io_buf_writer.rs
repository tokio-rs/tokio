#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

// https://github.com/rust-lang/futures-rs/blob/1803948ff091b4eabf7f3bf39e16bbbdefca5cc8/futures/tests/io_buf_writer.rs

use futures::task::{Context, Poll};
use std::io::{self, Cursor};
use std::pin::Pin;
use tokio::io::{AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufWriter, SeekFrom};

struct MaybePending {
    inner: Vec<u8>,
    ready: bool,
}

impl MaybePending {
    fn new(inner: Vec<u8>) -> Self {
        Self {
            inner,
            ready: false,
        }
    }
}

impl AsyncWrite for MaybePending {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.ready {
            self.ready = false;
            Pin::new(&mut self.inner).poll_write(cx, buf)
        } else {
            self.ready = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[tokio::test]
async fn buf_writer() {
    let mut writer = BufWriter::with_capacity(2, Vec::new());

    writer.write(&[0, 1]).await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(*writer.get_ref(), [0, 1]);

    writer.write(&[2]).await.unwrap();
    assert_eq!(writer.buffer(), [2]);
    assert_eq!(*writer.get_ref(), [0, 1]);

    writer.write(&[3]).await.unwrap();
    assert_eq!(writer.buffer(), [2, 3]);
    assert_eq!(*writer.get_ref(), [0, 1]);

    writer.flush().await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(*writer.get_ref(), [0, 1, 2, 3]);

    writer.write(&[4]).await.unwrap();
    writer.write(&[5]).await.unwrap();
    assert_eq!(writer.buffer(), [4, 5]);
    assert_eq!(*writer.get_ref(), [0, 1, 2, 3]);

    writer.write(&[6]).await.unwrap();
    assert_eq!(writer.buffer(), [6]);
    assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5]);

    writer.write(&[7, 8]).await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5, 6, 7, 8]);

    writer.write(&[9, 10, 11]).await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);

    writer.flush().await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
}

#[tokio::test]
async fn buf_writer_inner_flushes() {
    let mut w = BufWriter::with_capacity(3, Vec::new());
    w.write(&[0, 1]).await.unwrap();
    assert_eq!(*w.get_ref(), []);
    w.flush().await.unwrap();
    let w = w.into_inner();
    assert_eq!(w, [0, 1]);
}

#[tokio::test]
async fn buf_writer_seek() {
    let mut w = BufWriter::with_capacity(3, Cursor::new(Vec::new()));
    w.write_all(&[0, 1, 2, 3, 4, 5]).await.unwrap();
    w.write_all(&[6, 7]).await.unwrap();
    assert_eq!(w.seek(SeekFrom::Current(0)).await.unwrap(), 8);
    assert_eq!(&w.get_ref().get_ref()[..], &[0, 1, 2, 3, 4, 5, 6, 7][..]);
    assert_eq!(w.seek(SeekFrom::Start(2)).await.unwrap(), 2);
    w.write_all(&[8, 9]).await.unwrap();
    w.flush().await.unwrap();
    assert_eq!(&w.into_inner().into_inner()[..], &[0, 1, 8, 9, 4, 5, 6, 7]);
}

#[tokio::test]
async fn maybe_pending_buf_writer() {
    let mut writer = BufWriter::with_capacity(2, MaybePending::new(Vec::new()));

    writer.write(&[0, 1]).await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(&writer.get_ref().inner, &[0, 1]);

    writer.write(&[2]).await.unwrap();
    assert_eq!(writer.buffer(), [2]);
    assert_eq!(&writer.get_ref().inner, &[0, 1]);

    writer.write(&[3]).await.unwrap();
    assert_eq!(writer.buffer(), [2, 3]);
    assert_eq!(&writer.get_ref().inner, &[0, 1]);

    writer.flush().await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(&writer.get_ref().inner, &[0, 1, 2, 3]);

    writer.write(&[4]).await.unwrap();
    writer.write(&[5]).await.unwrap();
    assert_eq!(writer.buffer(), [4, 5]);
    assert_eq!(&writer.get_ref().inner, &[0, 1, 2, 3]);

    writer.write(&[6]).await.unwrap();
    assert_eq!(writer.buffer(), [6]);
    assert_eq!(writer.get_ref().inner, &[0, 1, 2, 3, 4, 5]);

    writer.write(&[7, 8]).await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(writer.get_ref().inner, &[0, 1, 2, 3, 4, 5, 6, 7, 8]);

    writer.write(&[9, 10, 11]).await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(
        writer.get_ref().inner,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    );

    writer.flush().await.unwrap();
    assert_eq!(writer.buffer(), []);
    assert_eq!(
        &writer.get_ref().inner,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    );
}

#[tokio::test]
async fn maybe_pending_buf_writer_inner_flushes() {
    let mut w = BufWriter::with_capacity(3, MaybePending::new(Vec::new()));
    w.write(&[0, 1]).await.unwrap();
    assert_eq!(&w.get_ref().inner, &[]);
    w.flush().await.unwrap();
    let w = w.into_inner().inner;
    assert_eq!(w, [0, 1]);
}

#[tokio::test]
async fn maybe_pending_buf_writer_seek() {
    struct MaybePendingSeek {
        inner: Cursor<Vec<u8>>,
        ready_write: bool,
        ready_seek: bool,
        seek_res: Option<io::Result<()>>,
    }

    impl MaybePendingSeek {
        fn new(inner: Vec<u8>) -> Self {
            Self {
                inner: Cursor::new(inner),
                ready_write: false,
                ready_seek: false,
                seek_res: None,
            }
        }
    }

    impl AsyncWrite for MaybePendingSeek {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.ready_write {
                self.ready_write = false;
                Pin::new(&mut self.inner).poll_write(cx, buf)
            } else {
                self.ready_write = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.inner).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.inner).poll_shutdown(cx)
        }
    }

    impl AsyncSeek for MaybePendingSeek {
        fn start_seek(mut self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
            self.seek_res = Some(Pin::new(&mut self.inner).start_seek(pos));
            Ok(())
        }
        fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            if self.ready_seek {
                self.ready_seek = false;
                self.seek_res.take().unwrap_or(Ok(()))?;
                Pin::new(&mut self.inner).poll_complete(cx)
            } else {
                self.ready_seek = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    let mut w = BufWriter::with_capacity(3, MaybePendingSeek::new(Vec::new()));
    w.write_all(&[0, 1, 2, 3, 4, 5]).await.unwrap();
    w.write_all(&[6, 7]).await.unwrap();
    assert_eq!(w.seek(SeekFrom::Current(0)).await.unwrap(), 8);
    assert_eq!(
        &w.get_ref().inner.get_ref()[..],
        &[0, 1, 2, 3, 4, 5, 6, 7][..]
    );
    assert_eq!(w.seek(SeekFrom::Start(2)).await.unwrap(), 2);
    w.write_all(&[8, 9]).await.unwrap();
    w.flush().await.unwrap();
    assert_eq!(
        &w.into_inner().inner.into_inner()[..],
        &[0, 1, 8, 9, 4, 5, 6, 7]
    );
}
