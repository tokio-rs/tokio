#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use futures_util::future::poll_fn;
use rand::{distributions, thread_rng, Rng};
use std::fs;
use std::io::SeekFrom;
use tempfile::Builder as TmpBuilder;
use tokio_fs::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn read_write() {
    const NUM_CHARS: usize = 16 * 1_024;

    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("read_write.txt");

    let contents: Vec<u8> = thread_rng()
        .sample_iter(&distributions::Alphanumeric)
        .take(NUM_CHARS)
        .collect::<String>()
        .into();

    let file_path = file_path.clone();
    let contents = contents.clone();

    let file = File::create(file_path.clone()).await.unwrap();
    let (mut file, metadata) = file.metadata().await.unwrap();
    assert!(metadata.is_file());
    assert!(file.write(&contents).await.is_ok());
    assert!(poll_fn(move |_cx| file.poll_sync_all()).await.is_ok());

    let dst = fs::read(&file_path).unwrap();
    assert_eq!(dst, contents);

    let buf = read(file_path).await.unwrap();
    assert_eq!(buf, contents);
}

#[tokio::test]
async fn read_write_helpers() {
    const NUM_CHARS: usize = 16 * 1_024;

    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("read_write_all.txt");

    let contents: Vec<u8> = thread_rng()
        .sample_iter(&distributions::Alphanumeric)
        .take(NUM_CHARS)
        .collect::<String>()
        .into();

    assert!(write(file_path.clone(), contents.clone()).await.is_ok());

    let dst = fs::read(&file_path).unwrap();
    assert_eq!(dst, contents);

    let buf = read(file_path).await.unwrap();
    assert_eq!(buf, contents);
}

#[tokio::test]
async fn metadata() {
    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("metadata.txt");

    assert!(tokio_fs::metadata(file_path.clone()).await.is_err());
    assert!(File::create(file_path.clone()).await.is_ok());
    let metadata = tokio_fs::metadata(file_path.clone()).await.unwrap();
    assert!(metadata.is_file());
}

#[tokio::test]
async fn seek() {
    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("seek.txt");

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(file_path).await.unwrap();
    assert!(file.write(b"Hello, world!").await.is_ok());
    let mut file = file.seek(SeekFrom::End(-6)).await.unwrap().0;
    let mut buf = vec![0; 5];
    assert!(file.read(buf.as_mut()).await.is_ok());
    assert_eq!(buf, b"world");
    let mut file = file.seek(SeekFrom::Start(0)).await.unwrap().0;
    let mut buf = vec![0; 5];
    assert!(file.read(buf.as_mut()).await.is_ok());
    assert_eq!(buf, b"Hello");
}

#[tokio::test]
async fn clone() {
    use std::io::prelude::*;

    let dir = TmpBuilder::new()
        .prefix("tokio-fs-tests")
        .tempdir()
        .unwrap();
    let file_path = dir.path().join("clone.txt");

    let file = File::create(file_path.clone()).await.unwrap();
    let (mut file, mut clone) = file.try_clone().await.unwrap();
    assert!(AsyncWriteExt::write(&mut file, b"clone ").await.is_ok());
    assert!(AsyncWriteExt::write(&mut clone, b"successful").await.is_ok());

    let mut file = std::fs::File::open(&file_path).unwrap();

    let mut dst = vec![];
    file.read_to_end(&mut dst).unwrap();

    assert_eq!(dst, b"clone successful")
}
