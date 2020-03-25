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

#[tokio::test]
async fn copy_permissions() {
    let dir = tempdir().unwrap();
    let source_path = dir.path().join("foo.txt");
    let dest_path = dir.path().join("bar.txt");

    let source = tokio::fs::File::create(&source_path).await.unwrap();
    let mut source_perms = source.metadata().await.unwrap().permissions();
    source_perms.set_readonly(true);
    source.set_permissions(source_perms.clone()).await.unwrap();

    tokio::fs::copy(source_path, &dest_path).await.unwrap();

    let dest_perms = tokio::fs::metadata(dest_path).await.unwrap().permissions();

    assert_eq!(source_perms, dest_perms);
}
