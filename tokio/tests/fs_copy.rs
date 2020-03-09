#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn copy() {
    let dir = tempdir().unwrap();

    let source_path = dir.path().join("foo.txt");
    let dest_path = dir.path().join("bar.txt");

    fs::write(&source_path, b"Hello File!").await.unwrap();
    fs::copy(&source_path, &dest_path).await.unwrap();

    let from = fs::read(&source_path).await.unwrap();
    let to = fs::read(&dest_path).await.unwrap();

    assert_eq!(from, to);
}
