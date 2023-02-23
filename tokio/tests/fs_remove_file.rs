#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(tokio_wasi)))] // WASI does not support all fs operations

use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn remove_file() {
    let temp_dir = tempdir().unwrap();

    let file_path = temp_dir.path().join("a.txt");

    fs::write(&file_path, b"Hello File!").await.unwrap();

    assert!(fs::try_exists(&file_path).await.unwrap());

    fs::remove_file(&file_path).await.unwrap();

    // should no longer exist
    match fs::try_exists(file_path).await {
        Ok(exists) => assert!(!exists),
        Err(info) => println!("ignoring error after remove, see info: {:?}", info),
    };
}
