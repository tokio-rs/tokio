extern crate futures;
extern crate tempdir;
extern crate tokio_fs;

use futures::{Future, Stream};
use std::fs;
use std::sync::{Arc, Mutex};
use tempdir::TempDir;
use tokio_fs::*;

mod pool;

#[test]
fn create() {
    let base_dir = TempDir::new("base").unwrap();
    let new_dir = base_dir.path().join("foo");

    pool::run({ create_dir(new_dir.clone()) });

    assert!(new_dir.is_dir());
}

#[test]
fn create_all() {
    let base_dir = TempDir::new("base").unwrap();
    let new_dir = base_dir.path().join("foo").join("bar");

    pool::run({ create_dir_all(new_dir.clone()) });

    assert!(new_dir.is_dir());
}

#[test]
fn remove() {
    let base_dir = TempDir::new("base").unwrap();
    let new_dir = base_dir.path().join("foo");

    fs::create_dir(new_dir.clone()).unwrap();

    pool::run({ remove_dir(new_dir.clone()) });

    assert!(!new_dir.exists());
}

#[test]
fn read() {
    let base_dir = TempDir::new("base").unwrap();

    let p = base_dir.path();
    fs::create_dir(p.join("aa")).unwrap();
    fs::create_dir(p.join("bb")).unwrap();
    fs::create_dir(p.join("cc")).unwrap();

    let files = Arc::new(Mutex::new(Vec::new()));

    let f = files.clone();
    let p = p.to_path_buf();
    pool::run({
        read_dir(p).flatten_stream().for_each(move |e| {
            let s = e.file_name().to_str().unwrap().to_string();
            f.lock().unwrap().push(s);
            Ok(())
        })
    });

    let mut files = files.lock().unwrap();
    files.sort(); // because the order is not guaranteed
    assert_eq!(
        *files,
        vec!["aa".to_string(), "bb".to_string(), "cc".to_string()]
    );
}
