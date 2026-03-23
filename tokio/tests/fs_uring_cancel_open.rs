//! Uring file operations tests.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use futures::future::FutureExt;
use std::fs;
use std::task::Poll;
use std::{future::poll_fn, path::PathBuf};
use tempfile::NamedTempFile;
use tokio_test::assert_pending;

// see: https://github.com/tokio-rs/tokio/issues/7979
#[tokio::test]
async fn file_descriptors_are_closed_when_cancelling_open_op() {
    let (_tmp_file, path): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(1);

    let fd_count_before_access = fs::read_dir("/proc/self/fd").unwrap().count();

    for _ in 0..128 {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let path = path.clone();
        let handle = tokio::spawn(async move {
            poll_fn(|cx| {
                let opt = {
                    let mut opt = tokio::fs::OpenOptions::new();
                    opt.read(true);
                    opt
                };

                let fut = opt.open(&path[0]);

                // If io_uring is enabled (and not falling back to the thread pool),
                // the first poll should return Pending.
                assert_pending!(Box::pin(fut).poll_unpin(cx));

                tx.send(()).unwrap();

                Poll::<()>::Pending
            })
            .await;
        });

        // Wait for the first poll
        rx.recv().await.unwrap();

        handle.abort();

        let res = handle.await.unwrap_err();
        assert!(res.is_cancelled());
    }

    let fd_count_after_cancel = fs::read_dir("/proc/self/fd").unwrap().count();
    let leaked = fd_count_after_cancel.saturating_sub(fd_count_before_access);

    //TODO(7979): check and explain
    assert!(leaked <= 64);
}

fn create_tmp_files(num_files: usize) -> (Vec<NamedTempFile>, Vec<PathBuf>) {
    let mut files = Vec::with_capacity(num_files);
    for _ in 0..num_files {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        files.push((tmp, path));
    }

    files.into_iter().unzip()
}
