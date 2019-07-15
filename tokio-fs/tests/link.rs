extern crate futures;
extern crate tempfile;
extern crate tokio_fs;

use futures::Future;
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

    pool::run({ hard_link(src, dst.clone()) });

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

    pool::run({ os::unix::symlink(src.clone(), dst.clone()) });

    let mut content = String::new();

    {
        let file = fs::File::open(dst.clone()).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert!(content == "hello");

    pool::run({ read_link(dst.clone()).map(move |x| assert!(x == src)) });
    pool::run({ symlink_metadata(dst.clone()).map(move |x| assert!(x.file_type().is_symlink())) });
}
