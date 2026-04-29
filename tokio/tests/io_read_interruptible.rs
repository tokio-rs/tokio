#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::future;
use std::io::SeekFrom;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[tokio::test]
async fn read_not_interrupted() {
    let mut buf = [0u8; 8];
    let mut interruptible = io::repeat(0xAB).read_interruptible(future::pending::<()>());
    let n = interruptible.read(&mut buf).await.unwrap();
    assert!(n > 0);
    assert!(buf[..n].iter().all(|&b| b == 0xAB));
    assert!(!interruptible.is_stopped());
}

#[tokio::test]
async fn read_interrupted() {
    let mut buf = [0u8; 8];
    let mut interruptible = io::repeat(0xAB).read_interruptible(future::ready(()));
    let n = interruptible.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
    assert!(interruptible.is_stopped());
}

#[tokio::test]
async fn interrupted_read_stays_interrupted() {
    let mut buf = [0u8; 8];
    let mut interruptible = io::repeat(0xAB).read_interruptible(future::ready(()));

    let n = interruptible.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
    assert!(interruptible.is_stopped());

    // Second and third reads must also return EOF, not data from the inner reader.
    let n = interruptible.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
    let n = interruptible.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn buf_read_not_interrupted() {
    let data: &[u8] = b"hello world";
    let mut interruptible = data.read_interruptible(future::pending::<()>());
    let filled = interruptible.fill_buf().await.unwrap();
    assert_eq!(filled, b"hello world");
    assert!(!interruptible.is_stopped());
}

#[tokio::test]
async fn buf_read_interrupted() {
    let data: &[u8] = b"hello world";
    let mut interruptible = data.read_interruptible(future::ready(()));
    let filled = interruptible.fill_buf().await.unwrap();
    assert!(filled.is_empty());
    assert!(interruptible.is_stopped());
}

#[tokio::test]
async fn interrupted_buf_read_stays_interrupted() {
    let data: &[u8] = b"hello world";
    let mut interruptible = data.read_interruptible(future::ready(()));

    let filled = interruptible.fill_buf().await.unwrap();
    assert!(filled.is_empty());
    assert!(interruptible.is_stopped());

    // Subsequent fill_buf calls must also return empty, not data from the inner reader.
    let filled = interruptible.fill_buf().await.unwrap();
    assert!(filled.is_empty());
    let filled = interruptible.fill_buf().await.unwrap();
    assert!(filled.is_empty());
}

#[tokio::test]
async fn write_never_interrupted() {
    let cursor = std::io::Cursor::new(Vec::<u8>::new());
    let mut interruptible = cursor.read_interruptible(future::ready(()));

    // Reading is interrupted.
    let mut buf = [0u8; 1];
    let n = interruptible.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
    assert!(interruptible.is_stopped());

    // Writing still works.
    let written = interruptible.write(b"hello").await.unwrap();
    assert_eq!(written, 5);
    assert_eq!(interruptible.get_ref().get_ref(), b"hello");
}

#[tokio::test]
async fn seek_never_interrupted() {
    let cursor = std::io::Cursor::new(b"hello world".to_vec());
    let mut interruptible = cursor.read_interruptible(future::ready(()));

    // Reading is interrupted.
    let mut buf = [0u8; 1];
    let n = interruptible.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
    assert!(interruptible.is_stopped());

    // Seeking still works.
    let pos = interruptible.seek(SeekFrom::Start(6)).await.unwrap();
    assert_eq!(pos, 6);
}
