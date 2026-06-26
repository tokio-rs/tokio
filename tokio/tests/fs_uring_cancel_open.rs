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
use std::future::poll_fn;
use std::task::Poll;
use tempfile::NamedTempFile;
use tokio_test::assert_pending;

use crate::support::io_uring::{assert_fds_are_not_leaking, io_uring_supported};

mod support {
    pub(crate) mod io_uring;
}

// see: https://github.com/tokio-rs/tokio/issues/7979
#[tokio::test]
async fn file_descriptors_are_closed_when_cancelling_open_op() {
    if !io_uring_supported() {
        return;
    }

    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    let file_number = 128;
    let fd_count_before_opens = fs::read_dir("/proc/self/fd").unwrap().count();

    for _ in 0..file_number {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let path = path.clone();
        let handle = tokio::spawn(async move {
            poll_fn(|cx| {
                let opt = {
                    let mut opt = tokio::fs::OpenOptions::new();
                    opt.read(true);
                    opt
                };

                let fut = opt.open(&path);

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

    assert_fds_are_not_leaking(fd_count_before_opens, file_number, 1).await
}
