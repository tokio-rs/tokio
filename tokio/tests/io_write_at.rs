#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))]

#[tokio::test]
#[cfg(unix)]
async fn write_at() {
    use tempfile::tempdir;
    use tokio::fs;
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncSeekExt;

    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("a.txt");
    fs::write(&file_path, b"Hello File").await.unwrap();
    let mut file = OpenOptions::new()
        .write(true)
        .open(file_path.as_path())
        .await
        .unwrap();

    assert_eq!(file.write_at(b"World", 5).await.unwrap(), 5);
    let contents = fs::read(file_path.as_path()).await.unwrap();
    assert_eq!(contents, b"HelloWorld");

    assert_eq!(file.stream_position().await.unwrap(), 0);
}
