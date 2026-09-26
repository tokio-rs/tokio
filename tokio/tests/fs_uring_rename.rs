//! Uring file operations tests.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use futures::future::Future;
use futures::future::FutureExt;
use libc::PATH_MAX;
use std::future::poll_fn;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;
use tempfile::{tempdir, NamedTempFile};
use tokio::fs::{rename, try_exists};
use tokio::runtime::{Builder, Runtime};
use tokio_test::assert_pending;
use tokio_util::task::TaskTracker;

use crate::support::io_uring::io_uring_supported;

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
    if !io_uring_supported() {
        return;
    }

    fn run(rt: Runtime) {
        let (done_tx, done_rx) = mpsc::channel();
        let (_tmp, path) = create_tmp_files(1);
        // keep 100 permits
        const N: i32 = 100;
        rt.spawn(async move {
            let path = path[0].clone();
            let dst = path.with_extension("renamed");

            // spawning a bunch of uring operations.
            let mut futs = vec![];

            // spawning a bunch of uring operations.
            for _ in 0..N {
                let mut fut = Box::pin(rename(path.clone(), dst.clone()));

                poll_fn(|cx| {
                    assert_pending!(fut.as_mut().poll(cx));
                    Poll::<()>::Pending
                })
                .await;

                futs.push(fut);
            }

            tokio::task::yield_now().await;
        });

        std::thread::spawn(move || {
            rt.shutdown_timeout(Duration::from_millis(300));
            done_tx.send(()).unwrap();
        });

        done_rx.recv().unwrap();
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[test]
fn rename_many_files() {
    fn run(rt: Runtime) {
        const NUM_FILES: usize = 512;

        let dir = tempdir().unwrap();

        rt.block_on(async move {
            let tracker = TaskTracker::new();

            for i in 0..NUM_FILES {
                let src = dir.path().join(format!("src-{i}"));
                let dst = dir.path().join(format!("dst-{i}"));
                tokio::fs::write(&src, b"contents").await.unwrap();

                tracker.spawn(async move {
                    rename(&src, &dst).await.unwrap();

                    assert!(!try_exists(&src).await.unwrap());
                    assert!(try_exists(&dst).await.unwrap());
                });
            }
            tracker.close();
            tracker.wait().await;
        });
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn rename_file() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    tokio::fs::write(&src, b"Hello File!").await.unwrap();

    rename(&src, &dst).await.unwrap();

    assert!(!try_exists(&src).await.unwrap());
    assert!(try_exists(&dst).await.unwrap());
    assert_eq!(tokio::fs::read(&dst).await.unwrap(), b"Hello File!");
}

#[tokio::test]
async fn rename_replaces_destination() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");

    tokio::fs::write(&src, b"source").await.unwrap();
    tokio::fs::write(&dst, b"destination").await.unwrap();

    rename(&src, &dst).await.unwrap();

    assert!(!try_exists(&src).await.unwrap());
    assert_eq!(tokio::fs::read(&dst).await.unwrap(), b"source");
}

#[tokio::test]
async fn rename_directory() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src_dir");
    let dst = dir.path().join("dst_dir");

    tokio::fs::create_dir(&src).await.unwrap();
    tokio::fs::write(src.join("file.txt"), b"contents")
        .await
        .unwrap();

    rename(&src, &dst).await.unwrap();

    assert!(!try_exists(&src).await.unwrap());
    assert!(try_exists(&dst).await.unwrap());
    assert_eq!(
        tokio::fs::read(dst.join("file.txt")).await.unwrap(),
        b"contents"
    );
}

#[tokio::test]
async fn rename_nonexistent_source() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("nonexistent");
    let dst = dir.path().join("dst");

    let result = rename(&src, &dst).await;
    assert_eq!(result.err().unwrap().raw_os_error().unwrap(), libc::ENOENT);
}

#[tokio::test]
async fn rename_path_name_too_long() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src.txt");
    tokio::fs::write(&src, b"Hello File!").await.unwrap();

    // if the destination name is above PATH_MAX (Linux is 4096 bytes), we should
    // receive a `std::io::ErrorKind::InvalidFilename` error.
    let long_dst = dir.path().join(vec!["a"; (PATH_MAX + 1) as usize].join(""));
    let result = rename(&src, &long_dst).await;
    assert_eq!(
        result.err().unwrap().kind(),
        std::io::ErrorKind::InvalidFilename
    );
}

#[tokio::test]
async fn cancel_op_future() {
    if !io_uring_supported() {
        return;
    }

    let (_tmp_file, path): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(1);
    let path = path[0].clone();
    let dst = path.with_extension("renamed");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let fut = rename(path.clone(), dst.clone());

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

fn create_tmp_files(num_files: usize) -> (Vec<NamedTempFile>, Vec<PathBuf>) {
    let mut files = Vec::with_capacity(num_files);
    for _ in 0..num_files {
        let mut tmp = NamedTempFile::new().unwrap();
        let buf = vec![20; 1023];
        tmp.write_all(&buf).unwrap();
        let path = tmp.path().to_path_buf();
        files.push((tmp, path));
    }

    files.into_iter().unzip()
}
