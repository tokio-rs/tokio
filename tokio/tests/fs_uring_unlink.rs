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
use tokio::fs::{create_dir, remove_dir, remove_file, try_exists, write};
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

            // spawning a bunch of uring operations.
            let mut futs = vec![];

            // spawning a bunch of uring operations.
            for _ in 0..N {
                let path = path.clone();
                let mut fut = Box::pin(remove_file(path));

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
fn remove_many_files() {
    fn run(rt: Runtime) {
        const NUM_FILES: usize = 512;

        let (_tmp_files, paths): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(NUM_FILES);

        rt.block_on(async move {
            let tracker = TaskTracker::new();

            for path in paths {
                tracker.spawn(async move {
                    remove_file(&path).await.unwrap();
                    assert!(!try_exists(&path).await.unwrap());
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
async fn remove_existing_file() {
    let (tmp, path) = create_temp_file();

    remove_file(&path).await.unwrap();
    assert!(!try_exists(&path).await.unwrap());

    tmp.into_temp_path().disable_cleanup(true);
}

#[tokio::test]
async fn remove_existing_dir() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("to_remove");
    create_dir(&path).await.unwrap();
    assert!(try_exists(&path).await.unwrap());

    remove_dir(&path).await.unwrap();
    assert!(!try_exists(&path).await.unwrap());
}

#[tokio::test]
async fn remove_nonexistent_file() {
    let path = tempdir().unwrap().path().join("nonexistent_path");
    let result = remove_file(&path).await;
    assert_eq!(result.err().unwrap().raw_os_error().unwrap(), libc::ENOENT);
}

#[tokio::test]
async fn remove_nonexistent_dir() {
    let path = tempdir().unwrap().path().join("nonexistent_dir");
    let result = remove_dir(&path).await;
    assert_eq!(result.err().unwrap().raw_os_error().unwrap(), libc::ENOENT);
}

#[tokio::test]
async fn remove_file_on_directory() {
    // Calling `remove_file` on a directory should fail as it does not pass
    // `AT_REMOVEDIR`.
    let dir = tempdir().unwrap();
    let path = dir.path().join("a_dir");
    create_dir(&path).await.unwrap();

    let result = remove_file(&path).await;
    let errno = result.err().unwrap().raw_os_error().unwrap();
    assert!(errno == libc::EISDIR || errno == libc::EPERM);

    // The directory should still exist.
    assert!(try_exists(&path).await.unwrap());
}

#[tokio::test]
async fn remove_dir_on_file() {
    // Calling `remove_dir` on a regular file should fail.
    let (_tmp, path) = create_temp_file();

    let result = remove_dir(&path).await;
    assert_eq!(result.err().unwrap().raw_os_error().unwrap(), libc::ENOTDIR);

    // The file should still exist.
    assert!(try_exists(&path).await.unwrap());
}

#[tokio::test]
async fn remove_non_empty_dir() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("non_empty");
    create_dir(&path).await.unwrap();
    write(path.join("file.txt"), b"Hello File!").await.unwrap();

    let result = remove_dir(&path).await;
    assert_eq!(
        result.err().unwrap().raw_os_error().unwrap(),
        libc::ENOTEMPTY
    );
}

#[tokio::test]
async fn remove_path_name_too_long() {
    let dir = tempdir().unwrap();
    // If we unlink a file whose name is above PATH_MAX (Linux is 4096 bytes),
    // we should receive an `std::io::ErrorKind::InvalidFilename` error.
    let long_nonexistent_path = dir.path().join(vec!["a"; (PATH_MAX + 1) as usize].join(""));
    let result = remove_file(long_nonexistent_path).await;
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

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let fut = remove_file(path.clone());

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

fn create_temp_file() -> (NamedTempFile, PathBuf) {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"Hello File!").unwrap();
    let path = tmp.path().to_path_buf();

    (tmp, path)
}
