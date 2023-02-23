#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(tokio_wasi)))] // WASI does not support all fs operations

use tokio::fs;

use std::io::prelude::*;
use std::io::BufReader;
use tempfile::tempdir;

#[tokio::test]
async fn test_hard_link() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = std::fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    let dst_2 = dst.clone();

    assert!(fs::hard_link(src.clone(), dst_2.clone()).await.is_ok());

    {
        let mut file = std::fs::File::create(&src).unwrap();
        file.write_all(b"new-data").unwrap();
    }

    let mut content = String::new();

    {
        let file = std::fs::File::open(dst).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert_eq!(content, "new-data");

    // test that this is not a symlink:
    assert!(fs::read_link(dst_2.clone()).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn test_symlink() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = std::fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    let src_2 = src.clone();
    let dst_2 = dst.clone();

    assert!(fs::symlink(src_2.clone(), dst_2.clone()).await.is_ok());

    {
        let mut file = std::fs::File::create(&src).unwrap();
        file.write_all(b"new-data").unwrap();
    }

    let mut content = String::new();

    {
        let file = std::fs::File::open(dst.clone()).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert_eq!(content, "new-data");

    let read = fs::read_link(dst.clone()).await.unwrap();
    assert!(read == src);

    let symlink_meta = fs::symlink_metadata(dst.clone()).await.unwrap();
    assert!(symlink_meta.file_type().is_symlink());
}
