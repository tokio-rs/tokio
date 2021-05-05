#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_test::task;

use std::io::prelude::*;
use tempfile::NamedTempFile;

const HELLO: &[u8] = b"hello world...";

#[tokio::test]
async fn basic_read() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut file = File::open(tempfile.path()).await.unwrap();

    let mut buf = [0; 1024];
    let n = file.read(&mut buf).await.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);

    // Drop the data from the cache to stimulate uncached codepath on Linux (see preadv2 in
    // file.rs)
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        nix::unistd::fsync(tempfile.as_raw_fd()).unwrap();
        nix::fcntl::posix_fadvise(
            tempfile.as_raw_fd(),
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
        )
        .unwrap();
    }

    let mut file = File::open(tempfile.path()).await.unwrap();
    let n = file.read(&mut buf).await.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[tokio::test]
async fn basic_write() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.write_all(HELLO).await.unwrap();
    file.flush().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, HELLO);
}

#[tokio::test]
async fn basic_write_and_shutdown() {
    let tempfile = tempfile();

    let mut file = File::create(tempfile.path()).await.unwrap();

    file.write_all(HELLO).await.unwrap();
    file.shutdown().await.unwrap();

    let file = std::fs::read(tempfile.path()).unwrap();
    assert_eq!(file, HELLO);
}

#[tokio::test]
async fn coop() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    let mut task = task::spawn(async {
        let mut file = File::open(tempfile.path()).await.unwrap();

        let mut buf = [0; 1024];

        loop {
            file.read(&mut buf).await.unwrap();
            file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        }
    });

    for _ in 0..1_000 {
        if task.poll().is_pending() {
            return;
        }
    }

    panic!("did not yield");
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

#[tokio::test]
#[cfg(unix)]
async fn unix_fd() {
    use std::os::unix::io::AsRawFd;
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    assert!(file.as_raw_fd() as u64 > 0);
}

#[tokio::test]
#[cfg(windows)]
async fn windows_handle() {
    use std::os::windows::io::AsRawHandle;
    let tempfile = tempfile();

    let file = File::create(tempfile.path()).await.unwrap();
    assert!(file.as_raw_handle() as u64 > 0);
}
