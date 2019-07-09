#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use futures_util::future;
use futures_util::try_stream::TryStreamExt;
use std::fs;
use std::sync::{Arc, Mutex};
use tempdir::TempDir;
use tokio_fs::*;

#[tokio::test]
async fn create() {
    let base_dir = TempDir::new("base").unwrap();
    let new_dir = base_dir.path().join("foo");

    assert!(create_dir(new_dir.clone()).await.is_ok());

    assert!(new_dir.is_dir());
}

#[tokio::test]
async fn create_all() {
    let base_dir = TempDir::new("base").unwrap();
    let new_dir = base_dir.path().join("foo").join("bar");

    assert!(create_dir_all(new_dir.clone()).await.is_ok());

    assert!(new_dir.is_dir());
}

#[tokio::test]
async fn remove() {
    let base_dir = TempDir::new("base").unwrap();
    let new_dir = base_dir.path().join("foo");

    fs::create_dir(new_dir.clone()).unwrap();

    assert!(remove_dir(new_dir.clone()).await.is_ok());

    assert!(!new_dir.exists());
}

#[tokio::test]
async fn read() {
    let base_dir = TempDir::new("base").unwrap();

    let p = base_dir.path();
    fs::create_dir(p.join("aa")).unwrap();
    fs::create_dir(p.join("bb")).unwrap();
    fs::create_dir(p.join("cc")).unwrap();

    let files = Arc::new(Mutex::new(Vec::new()));

    let f = files.clone();
    let p = p.to_path_buf();

    let read_dir_fut = read_dir(p).await.unwrap();
    assert!(read_dir_fut
        .try_for_each(move |e| {
            let s = e.file_name().to_str().unwrap().to_string();
            f.lock().unwrap().push(s);
            future::ok(())
        })
        .await
        .is_ok());

    let mut files = files.lock().unwrap();
    files.sort(); // because the order is not guaranteed
    assert_eq!(
        *files,
        vec!["aa".to_string(), "bb".to_string(), "cc".to_string()]
    );
}
