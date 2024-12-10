#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))]

#[tokio::test]
#[cfg(unix)]
async fn read_at() {
    use tempfile::tempdir;
    use tokio::fs;
    use tokio::io::AsyncSeekExt;

    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("a.txt");
    fs::write(&file_path, b"HelloWorld").await.unwrap();
    let mut file = fs::File::open(file_path.as_path()).await.unwrap();

    let mut buf = [0_u8; 10];
    assert_eq!(file.read_at(&mut buf, 5).await.unwrap(), 5);
    assert_eq!(&buf[..5], b"World");

    assert_eq!(file.stream_position().await.unwrap(), 0);
}
