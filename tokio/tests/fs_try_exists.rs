#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::os::unix::prelude::PermissionsExt;

use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn try_exists() {
    let dir = tempdir().unwrap();

    let existing_path = dir.path().join("foo.txt");
    fs::write(&existing_path, b"Hello File!").await.unwrap();
    let nonexisting_path = dir.path().join("bar.txt");

    assert_eq!(fs::try_exists(existing_path).await.unwrap(), true);
    assert_eq!(fs::try_exists(nonexisting_path).await.unwrap(), false);

    let permission_denied_directory_path = dir.path().join("baz");
    fs::create_dir(&permission_denied_directory_path)
        .await
        .unwrap();
    let permission_denied_file_path = permission_denied_directory_path.join("baz.txt");
    fs::write(&permission_denied_file_path, b"Hello File!")
        .await
        .unwrap();
    let mut perms = tokio::fs::metadata(&permission_denied_directory_path)
        .await
        .unwrap()
        .permissions();
    perms.set_mode(0o244);
    fs::set_permissions(&permission_denied_directory_path, perms)
        .await
        .unwrap();
    let permission_denied_result = fs::try_exists(permission_denied_file_path).await;
    assert_eq!(
        permission_denied_result.err().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied
    );
}
