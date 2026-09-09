//! Tests for io-uring filesystem fallback without a runtime IO driver.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use std::future::poll_fn;
use std::io::Write;
use std::pin::Pin;

use tempfile::{tempdir, NamedTempFile};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::runtime::{Builder, Runtime};

fn runtime_without_io() -> Runtime {
    Builder::new_current_thread().build().unwrap()
}

#[test]
fn fs_read_should_fall_back_to_blocking_if_runtime_has_no_io_driver() {
    let mut tempfile = NamedTempFile::new().unwrap();
    tempfile.write_all(b"hello").unwrap();

    let contents = runtime_without_io()
        .block_on(tokio::fs::read(tempfile.path()))
        .unwrap();

    assert_eq!(contents, b"hello");
}

#[test]
fn fs_write_should_fall_back_to_blocking_if_runtime_has_no_io_driver() {
    let tempfile = NamedTempFile::new().unwrap();

    runtime_without_io()
        .block_on(tokio::fs::write(tempfile.path(), b"hello"))
        .unwrap();

    let contents = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(contents, b"hello");
}

#[test]
fn open_options_should_fall_back_to_blocking_if_runtime_has_no_io_driver() {
    let mut tempfile = NamedTempFile::new().unwrap();
    tempfile.write_all(b"hello").unwrap();

    let file = runtime_without_io()
        .block_on(
            tokio::fs::OpenOptions::new()
                .read(true)
                .open(tempfile.path()),
        )
        .unwrap();

    drop(file);
}

#[test]
fn file_read_should_fall_back_to_blocking_if_runtime_has_no_io_driver() {
    let mut tempfile = NamedTempFile::new().unwrap();
    tempfile.write_all(b"hello").unwrap();

    let std_file = std::fs::File::open(tempfile.path()).unwrap();
    let mut file = tokio::fs::File::from_std(std_file);
    let mut contents = [0; 5];

    let n = runtime_without_io().block_on(async {
        let mut buf = ReadBuf::new(&mut contents);
        poll_fn(|cx| Pin::new(&mut file).poll_read(cx, &mut buf)).await?;
        std::io::Result::Ok(buf.filled().len())
    });

    assert_eq!(&contents[..n.unwrap()], b"hello");
}

#[test]
fn try_exists_should_fall_back_to_blocking_if_runtime_has_no_io_driver() {
    let tempfile = NamedTempFile::new().unwrap();
    let missing_path = tempfile.path().with_extension("missing");

    let exists = runtime_without_io()
        .block_on(tokio::fs::try_exists(tempfile.path()))
        .unwrap();
    let missing = runtime_without_io()
        .block_on(tokio::fs::try_exists(missing_path))
        .unwrap();

    assert!(exists);
    assert!(!missing);
}

#[test]
fn rename_should_fall_back_to_blocking_if_runtime_has_no_io_driver() {
    let temp_dir = tempdir().unwrap();
    let source_path = temp_dir.path().join("source");
    let renamed_path = temp_dir.path().join("renamed");
    std::fs::write(&source_path, b"hello").unwrap();

    runtime_without_io()
        .block_on(tokio::fs::rename(&source_path, &renamed_path))
        .unwrap();

    assert!(!source_path.exists());
    assert_eq!(std::fs::read(renamed_path).unwrap(), b"hello");
}
