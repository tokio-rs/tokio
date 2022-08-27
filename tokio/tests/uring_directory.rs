#[tokio::test]
async fn basic_remove_dir() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    tokio::platform::linux::uring::fs::remove_dir(temp_dir.path()).await.unwrap();
    assert!(std::fs::metadata(temp_dir.path()).is_err());
}
