#![cfg(feature = "compat")]
#![cfg(not(target_os = "wasi"))] // WASI does not support all fs operations
#![warn(rust_2018_idioms)]

use futures_io::SeekFrom;
use futures_util::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use std::io::{self, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll};
use tempfile::NamedTempFile;
use tokio::fs::OpenOptions;
use tokio_util::compat::TokioAsyncWriteCompatExt;

#[tokio::test]
async fn compat_file_seek() -> futures_util::io::Result<()> {
    let temp_file = NamedTempFile::new()?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(temp_file)
        .await?
        .compat_write();

    file.write_all(&[0, 1, 2, 3, 4, 5]).await?;
    file.write_all(&[6, 7]).await?;

    assert_eq!(file.stream_position().await?, 8);

    // Modify elements at position 2.
    assert_eq!(file.seek(SeekFrom::Start(2)).await?, 2);
    file.write_all(&[8, 9]).await?;

    file.flush().await?;

    // Verify we still have 8 elements.
    assert_eq!(file.seek(SeekFrom::End(0)).await?, 8);
    // Seek back to the start of the file to read and verify contents.
    file.seek(SeekFrom::Start(0)).await?;

    let mut buf = Vec::new();
    let num_bytes = file.read_to_end(&mut buf).await?;
    assert_eq!(&buf[..num_bytes], &[0, 1, 8, 9, 4, 5, 6, 7]);

    Ok(())
}

struct VectoredWriter {
    bufs: Vec<Vec<u8>>,
    result: Poll<Result<usize, io::ErrorKind>>,
    called: bool,
}

impl tokio::io::AsyncWrite for VectoredWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        panic!("vectored writes should not call poll_write");
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        assert!(bufs
            .iter()
            .map(|buf| &buf[..])
            .eq(self.bufs.iter().map(|buf| &buf[..])));
        self.called = true;
        if self.result.is_pending() {
            cx.waker().wake_by_ref();
        }
        self.result.map(|result| result.map_err(io::Error::from))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[test]
fn tokio_to_futures_vectored_write() {
    for bufs in [
        vec![],
        vec![IoSlice::new(b""), IoSlice::new(b"")],
        vec![IoSlice::new(b"abc"), IoSlice::new(b"def")],
        vec![
            IoSlice::new(b""),
            IoSlice::new(b"abc"),
            IoSlice::new(b""),
            IoSlice::new(b"def"),
            IoSlice::new(b""),
        ],
    ] {
        let len = bufs.iter().map(|buf| buf.len()).sum::<usize>();
        for result in [
            Poll::Ready(Ok(len)),
            Poll::Ready(Ok(len.min(4))),
            Poll::Ready(Ok(0)),
            Poll::Pending,
            Poll::Ready(Err(io::ErrorKind::BrokenPipe)),
        ] {
            let inner = VectoredWriter {
                bufs: bufs.iter().map(|buf| buf.to_vec()).collect(),
                result,
                called: false,
            };
            let mut writer = inner.compat_write();
            let mut task = tokio_test::task::spawn(());
            let actual = task.enter(|cx, _| {
                futures_io::AsyncWrite::poll_write_vectored(Pin::new(&mut writer), cx, &bufs)
            });
            assert_eq!(
                actual.map(|result| result.map_err(|err| err.kind())),
                result
            );
            assert!(writer.get_ref().called);
            assert_eq!(task.is_woken(), result.is_pending());
        }
    }
}

#[tokio::test]
async fn tokio_to_futures_vectored_write_scalar_fallback() {
    let inner = tokio_test::io::Builder::new().write(b"abc").build();
    let mut writer = inner.compat_write();
    let bufs = [
        IoSlice::new(b""),
        IoSlice::new(b"abc"),
        IoSlice::new(b"def"),
    ];
    assert_eq!(writer.write_vectored(&bufs).await.unwrap(), 3);
}
