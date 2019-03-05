#![deny(warnings, rust_2018_idioms)]

use crate::run;
use futures::Future;
use std::fs;
use std::io::prelude::*;
use std::io::BufReader;
use tempdir::TempDir;
use tokio_fs::*;

#[test]
fn test_hard_link() {
    let dir = TempDir::new("base").unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    run({ hard_link(src, dst.clone()) });

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
    let dir = TempDir::new("base").unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    {
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(b"hello").unwrap();
    }

    run({ os::unix::symlink(src.clone(), dst.clone()) });

    let mut content = String::new();

    {
        let file = fs::File::open(dst.clone()).unwrap();
        let mut reader = BufReader::new(file);
        reader.read_to_string(&mut content).unwrap();
    }

    assert!(content == "hello");

    run({ read_link(dst.clone()).map(move |x| assert!(x == src)) });
    run({ symlink_metadata(dst.clone()).map(move |x| assert!(x.file_type().is_symlink())) });
}
