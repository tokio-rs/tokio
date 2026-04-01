#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use std::io::{Read, Seek, SeekFrom};
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::runtime::Builder;

#[test]
fn test_sqpoll_current_thread() {
    let rt = Builder::new_current_thread()
        .enable_all()
        .uring_setup_sqpoll(1000)
        .build()
        .unwrap();

    rt.block_on(async {
        let mut temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();

        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .await
            .unwrap();

        file.write_all(b"hello").await.unwrap();
        file.flush().await.unwrap();

        // Check if data was actually written to the underlying file
        let mut buf = vec![0; 5];
        temp.as_file_mut().seek(SeekFrom::Start(0)).unwrap();
        temp.as_file_mut().read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");

        file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        let mut buf = vec![0; 5];
        file.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    });
}

#[test]
fn test_sqpoll_multi_thread() {
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .uring_setup_sqpoll(1000)
        .build()
        .unwrap();

    rt.block_on(async {
        let mut temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();

        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .await
            .unwrap();

        file.write_all(b"world").await.unwrap();
        file.flush().await.unwrap();

        // Check if data was actually written to the underlying file
        let mut buf = vec![0; 5];
        temp.as_file_mut().seek(SeekFrom::Start(0)).unwrap();
        temp.as_file_mut().read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"world");

        file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        let mut buf = vec![0; 5];
        file.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");
    });
}
