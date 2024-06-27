#![cfg(feature = "full")]
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt};
use tokio_test::assert_ok;

#[tokio::test]
async fn empty_read_is_cooperative() {
    tokio::select! {
        biased;

        _ = async {
            loop {
                let mut buf = [0u8; 4096];
                let _ = tokio::io::empty().read(&mut buf).await;
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

#[tokio::test]
async fn empty_buf_reads_are_cooperative() {
    tokio::select! {
        biased;

        _ = async {
            loop {
                let mut buf = String::new();
                let _ = tokio::io::empty().read_line(&mut buf).await;
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

#[tokio::test]
async fn empty_seek() {
    use std::io::SeekFrom;

    let mut empty = tokio::io::empty();

    let pos = assert_ok!(empty.seek(SeekFrom::Start(0)).await);
    assert_eq!(pos, 0);

    let pos = assert_ok!(empty.seek(SeekFrom::Start(8)).await);
    assert_eq!(pos, 0);
}
