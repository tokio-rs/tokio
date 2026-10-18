//! Uring hard link operations tests.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use futures::future::FutureExt;
use std::future::{pending, poll_fn};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;
use tempfile::{tempdir, TempDir};
use tokio::fs::hard_link;
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinSet;
use tokio_test::assert_pending;

use crate::support::io_uring::{io_uring_supported, uring_op_supported};

mod support {
    pub(crate) mod io_uring;
}

fn multi_rt(n: usize) -> Box<dyn Fn() -> Runtime> {
    Box::new(move || {
        Builder::new_multi_thread()
            .worker_threads(n)
            .enable_all()
            .build()
            .unwrap()
    })
}

fn current_rt() -> Box<dyn Fn() -> Runtime> {
    Box::new(|| Builder::new_current_thread().enable_all().build().unwrap())
}

fn rt_combinations() -> Vec<Box<dyn Fn() -> Runtime>> {
    vec![
        current_rt(),
        multi_rt(1),
        multi_rt(2),
        multi_rt(8),
        multi_rt(64),
        multi_rt(256),
    ]
}

#[test]
fn shutdown_runtime_while_performing_io_uring_ops() {
    if !uring_op_supported(io_uring::opcode::LinkAt::CODE) {
        return;
    }

    fn run(rt: Runtime) {
        let (done_tx, done_rx) = mpsc::channel();
        let (dir, target) = create_tmp_dir();
        const N: usize = 100;
        rt.block_on(async move {
            let mut futs = vec![];

            // spawning a bunch of uring operations.
            for i in 0..N {
                let link = dir.path().join(format!("link-{i}"));
                let mut fut = Box::pin(hard_link(target.clone(), link));

                assert_pending!(futures::poll!(fut.as_mut()));

                futs.push(fut);
            }

            tokio::spawn(async move {
                let _dir = dir;
                let _futs = futs;
                pending::<()>().await;
            });
        });

        std::thread::spawn(move || {
            rt.shutdown_timeout(Duration::from_millis(300));
            done_tx.send(()).unwrap();
        });

        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[test]
fn hard_link_many_files() {
    fn run(rt: Runtime) {
        const N_LINKS: usize = 512;

        let (dir, target) = create_tmp_dir();

        rt.block_on(async move {
            let target_metadata = tokio::fs::metadata(&target).await.unwrap();
            let mut tasks = JoinSet::new();

            for i in 0..N_LINKS {
                let target = target.clone();
                let link = dir.path().join(format!("link-{i}"));

                tasks.spawn(async move {
                    hard_link(&target, &link).await.unwrap();
                    tokio::fs::metadata(&link).await.unwrap()
                });
            }

            for link_metadata in tasks.join_all().await {
                assert_eq!(target_metadata.dev(), link_metadata.dev());
                assert_eq!(target_metadata.ino(), link_metadata.ino());
            }

            let target_metadata = tokio::fs::metadata(&target).await.unwrap();
            assert_eq!(target_metadata.nlink(), N_LINKS as u64 + 1);
        });
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn hard_link_file() {
    let (dir, target) = create_tmp_dir();
    let link = dir.path().join("link");

    hard_link(&target, &link).await.unwrap();

    tokio::fs::write(&target, b"Hello File!").await.unwrap();
    assert_eq!(tokio::fs::read(&link).await.unwrap(), b"Hello File!");
}

#[tokio::test]
async fn hard_link_existing_destination() {
    let (dir, target) = create_tmp_dir();
    let link = dir.path().join("link");
    tokio::fs::write(&link, b"existing").await.unwrap();

    let result = hard_link(&target, &link).await;
    assert_eq!(result.err().unwrap().raw_os_error().unwrap(), libc::EEXIST);
    assert_eq!(tokio::fs::read(&link).await.unwrap(), b"existing");
}

#[tokio::test]
async fn hard_link_nonexistent_source() {
    let dir = tempdir().unwrap();
    let original = dir.path().join("nonexistent");
    let link = dir.path().join("link");

    let result = hard_link(&original, &link).await;
    assert_eq!(result.err().unwrap().raw_os_error().unwrap(), libc::ENOENT);
}

// `std::fs::hard_link` links the symlink itself on Linux, the io_uring path
// has to do the same.
#[tokio::test]
async fn hard_link_does_not_follow_symlinks() {
    let (dir, target) = create_tmp_dir();
    let symlink = dir.path().join("symlink");
    let link = dir.path().join("link");
    tokio::fs::symlink(&target, &symlink).await.unwrap();

    hard_link(&symlink, &link).await.unwrap();

    let symlink_metadata = tokio::fs::symlink_metadata(&symlink).await.unwrap();
    let link_metadata = tokio::fs::symlink_metadata(&link).await.unwrap();
    assert!(link_metadata.file_type().is_symlink());
    assert_eq!(symlink_metadata.ino(), link_metadata.ino());
}

#[tokio::test]
async fn cancel_op_future() {
    if !io_uring_supported() {
        return;
    }

    let (dir, target) = create_tmp_dir();
    let link = dir.path().join("link");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let fut = hard_link(&target, &link);

            // the first poll should return Pending.
            assert_pending!(Box::pin(fut).poll_unpin(cx));
            tx.send(true).unwrap();

            Poll::<()>::Pending
        })
        .await;
    });

    // Wait for the first poll
    let val = rx.recv().await;
    assert!(val.unwrap());

    handle.abort();

    let res = handle.await.unwrap_err();
    assert!(res.is_cancelled());
}

fn create_tmp_dir() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    std::fs::write(&target, b"").unwrap();

    (dir, target)
}
