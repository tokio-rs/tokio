#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(tokio_wasi)))] // WASI does not support all fs operations

use std::io::prelude::*;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio_test::task;

const HELLO: &[u8] = b"hello world...";

#[tokio::test]
async fn basic_read() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut file = File::open(tempfile.path()).await.unwrap();

    let mut buf = [0; 1024];
    let n = file.read(&mut buf).await.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[tokio::test]
async fn basic_write() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.write_all(HELLO).await.unwrap();
    file.flush().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, HELLO);
}

#[tokio::test]
async fn basic_write_and_shutdown() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.write_all(HELLO).await.unwrap();
    file.shutdown().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, HELLO);
}

#[tokio::test]
async fn rewind_seek_position() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.seek(SeekFrom::Current(10)).await.unwrap();

    file.rewind().await.unwrap();

    assert_eq!(file.stream_position().await.unwrap(), 0);
}

#[tokio::test]
async fn coop() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut task = task::spawn(async {
        let mut file = File::open(tempfile.path()).await.unwrap();

        let mut buf = [0; 1024];

        loop {
            let _ = file.read(&mut buf).await.unwrap();
            file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        }
    });

    for _ in 0..1_000 {
        if task.poll().is_pending() {
            return;
        }
    }

    panic!("did not yield");
}

#[tokio::test]
async fn write_to_clone() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    let mut clone = file.try_clone().await.unwrap();

    clone.write_all(HELLO).await.unwrap();
    clone.flush().await.unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, HELLO);
}

#[tokio::test]
async fn write_into_std() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    let mut std_file = file.into_std().await;

    std_file.write_all(HELLO).unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, HELLO);
}

#[tokio::test]
async fn write_into_std_immediate() {
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    let mut std_file = file.try_into_std().unwrap();

    std_file.write_all(HELLO).unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, HELLO);
}

#[tokio::test]
async fn read_file_from_std() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let std_file = std::fs::File::open(tempfile.path()).unwrap();
    let mut file = File::from(std_file);

    let mut buf = [0; 1024];
    let n = file.read(&mut buf).await.unwrap();
    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

#[tokio::test]
#[cfg(unix)]
async fn file_debug_fmt() {
    let tempfile = tempfile();

    let file = File::open(tempfile.path()).await.unwrap();

    assert_eq!(
        &format!("{:?}", file)[0..33],
        "tokio::fs::File { std: File { fd:"
    );
}

#[tokio::test]
#[cfg(windows)]
async fn file_debug_fmt() {
    let tempfile = tempfile();

    let file = File::open(tempfile.path()).await.unwrap();

    assert_eq!(
        &format!("{:?}", file)[0..37],
        "tokio::fs::File { std: File { handle:"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn unix_fd_is_valid() {
    use std::os::unix::io::AsRawFd;
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    assert!(file.as_raw_fd() as u64 > 0);
}

#[tokio::test]
#[cfg(unix)]
async fn read_file_from_unix_fd() {
    use std::os::unix::io::AsRawFd;
    use std::os::unix::io::FromRawFd;

    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let file1 = File::open(tempfile.path()).await.unwrap();
    let raw_fd = file1.as_raw_fd();
    assert!(raw_fd > 0);

    let mut file2 = unsafe { File::from_raw_fd(raw_fd) };

    let mut buf = [0; 1024];
    let n = file2.read(&mut buf).await.unwrap();
    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[tokio::test]
#[cfg(windows)]
async fn windows_handle() {
    use std::os::windows::io::AsRawHandle;
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    assert!(file.as_raw_handle() as u64 > 0);
}
