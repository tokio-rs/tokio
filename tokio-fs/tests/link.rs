#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use std::fs;
use std::io::prelude::*;
use std::io::BufReader;
use tempdir::TempDir;
use tokio_fs::*;

#[tokio::test]
async fn test_hard_link() {
    let dir = TempDir::new("base").unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    assert!(hard_link(src, dst.clone()).await.is_ok());

    let mut content = String::new();

    {
        let file = fs::File::open(dst).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert!(content == "hello");
}

#[cfg(unix)]
#[tokio::test]
async fn test_symlink() {
    let dir = TempDir::new("base").unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    assert!(os::unix::symlink(src.clone(), dst.clone()).await.is_ok());

    let mut content = String::new();

    {
        let file = fs::File::open(dst.clone()).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert!(content == "hello");

    let read = read_link(dst.clone()).await.unwrap();
    assert!(read == src);
    
    let symlink_meta = symlink_metadata(dst.clone()).await.unwrap();
    assert!(symlink_meta.file_type().is_symlink());
}
