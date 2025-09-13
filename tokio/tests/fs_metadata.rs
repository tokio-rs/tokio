#![warn(rust_2018_idioms)]
#![cfg(all(
    tokio_uring,
    feature = "rt",
    feature = "fs",
    target_os = "linux",
    target_env = "gnu"
))]

use std::io::Write;
use tempfile::NamedTempFile;

use tokio::fs;

const HELLO: &[u8] = b"hello world...";

use std::os::linux::fs::MetadataExt;

#[tokio::test]
async fn metadata() {
    let mut tempfile = NamedTempFile::new().unwrap();
    tempfile.write_all(HELLO).unwrap();

    let got = fs::metadata(tempfile.path()).await.unwrap();
    let expected = std::fs::metadata(tempfile.path()).unwrap();

    assert_eq!(got.len(), expected.len());
    assert_eq!(got.created().unwrap(), expected.created().unwrap());
    assert_eq!(got.modified().unwrap(), expected.modified().unwrap());
    assert_eq!(got.st_size(), expected.st_size());
    assert_eq!(got.st_uid(), expected.st_uid());
}
