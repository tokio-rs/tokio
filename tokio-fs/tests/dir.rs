#![warn(rust_2018_idioms)]

use futures_util::future;
use futures_util::try_stream::TryStreamExt;
use std::fs;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio_fs::*;

mod pool;

#[test]
fn create() {
    let base_dir = tempdir().unwrap();
    let new_dir = base_dir.path().join("foo");
    let new_dir_2 = new_dir.clone();

    pool::run(async move {
        create_dir(new_dir).await?;
        Ok(())
    });

    assert!(new_dir_2.is_dir());
}

#[test]
fn create_all() {
    let base_dir = tempdir().unwrap();
    let new_dir = base_dir.path().join("foo").join("bar");
    let new_dir_2 = new_dir.clone();

    pool::run(async move {
        create_dir_all(new_dir).await?;
        Ok(())
    });

    assert!(new_dir_2.is_dir());
}

#[test]
fn remove() {
    let base_dir = tempdir().unwrap();
    let new_dir = base_dir.path().join("foo");
    let new_dir_2 = new_dir.clone();

    fs::create_dir(new_dir.clone()).unwrap();

    pool::run(async move {
        remove_dir(new_dir).await?;
        Ok(())
    });

    assert!(!new_dir_2.exists());
}

#[test]
fn read() {
    let base_dir = tempdir().unwrap();

    let p = base_dir.path();
    fs::create_dir(p.join("aa")).unwrap();
    fs::create_dir(p.join("bb")).unwrap();
    fs::create_dir(p.join("cc")).unwrap();

    let files = Arc::new(Mutex::new(Vec::new()));

    let f = files.clone();
    let p = p.to_path_buf();

    pool::run(async move {
        let read_dir_fut = read_dir(p).await?;
        read_dir_fut
            .try_for_each(move |e| {
                let s = e.file_name().to_str().unwrap().to_string();
                f.lock().unwrap().push(s);
                future::ok(())
            })
            .await?;
        Ok(())
    });

    let mut files = files.lock().unwrap();
    files.sort(); // because the order is not guaranteed
    assert_eq!(
        *files,
        vec!["aa".to_string(), "bb".to_string(), "cc".to_string()]
    );
}
