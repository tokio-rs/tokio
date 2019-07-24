#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use std::fs;
use std::io::prelude::*;
use std::io::BufReader;
use tempfile::tempdir;
use tokio_fs::*;

mod pool;

#[test]
fn test_hard_link() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    let dst_2 = dst.clone();

    pool::run(async move {
        assert!(hard_link(src, dst_2.clone()).await.is_ok());
        Ok(())
    });

    let mut content = String::new();

    {
        let file = fs::File::open(dst).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert!(content == "hello");
}

#[cfg(unix)]
#[test]
fn test_symlink() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    let src_2 = src.clone();
    let dst_2 = dst.clone();

    pool::run(async move {
        assert!(os::unix::symlink(src_2.clone(), dst_2.clone())
            .await
            .is_ok());
        Ok(())
    });

    let mut content = String::new();

    {
        let file = fs::File::open(dst.clone()).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert!(content == "hello");

    pool::run(async move {
        let read = read_link(dst.clone()).await.unwrap();
        assert!(read == src);

        let symlink_meta = symlink_metadata(dst.clone()).await.unwrap();
        assert!(symlink_meta.file_type().is_symlink());
        Ok(())
    });
}
