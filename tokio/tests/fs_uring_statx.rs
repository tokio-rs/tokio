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
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;
use tempfile::{tempdir, NamedTempFile};
use tokio::fs::{create_dir, metadata, set_permissions, symlink, try_exists, write};
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
                let mut fut = Box::pin(try_exists(path));

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
fn stat_many_files() {
    fn run(rt: Runtime) {
        const NUM_FILES: usize = 512;

        let (_tmp_files, paths): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(NUM_FILES);

        rt.block_on(async move {
            let tracker = TaskTracker::new();

            for i in 0..10_000 {
                let path = paths.get(i % NUM_FILES).unwrap().clone();
                tracker.spawn(async move {
                    let exists = try_exists(path).await.unwrap();
                    assert!(exists);
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
async fn stat_small_large_files() {
    let (_tmp, path) = create_large_temp_file();

    let exists = try_exists(path).await.unwrap();
    assert!(exists);

    let (_tmp, path) = create_small_temp_file();

    let exists = try_exists(path).await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn stat_nonexistent_file() {
    let path = tempdir().unwrap().path().join("nonexistent_path");
    let exists = try_exists(path).await.unwrap();
    assert!(!exists);
}

// Error is not produced on Linux 4.19 (Linux 7.1 it works)
#[tokio::test]
#[cfg_attr(miri, ignore)] // No `chmod` in miri.
#[cfg(unix)]
async fn stat_permission_denied() {
    if !io_uring_supported() {
        return;
    }

    let dir = tempdir().unwrap();
    let permission_denied_directory_path = dir.path().join("baz");
    create_dir(&permission_denied_directory_path).await.unwrap();
    let permission_denied_file_path = permission_denied_directory_path.join("baz.txt");
    write(&permission_denied_file_path, b"Hello File!")
        .await
        .unwrap();
    let mut perms = metadata(&permission_denied_directory_path)
        .await
        .unwrap()
        .permissions();

    perms.set_mode(0o244);
    set_permissions(&permission_denied_directory_path, perms)
        .await
        .unwrap();
    let permission_denied_result = try_exists(permission_denied_file_path).await;
    assert_eq!(
        permission_denied_result
            .err()
            .unwrap()
            .raw_os_error()
            .unwrap(),
        libc::EACCES
    );
}

#[tokio::test]
#[cfg(unix)]
async fn stat_filesystem_loop() {
    let dir = tempdir().unwrap();
    let first_symlink = dir.path().join("bar");
    let second_symlink = dir.path().join("foo");
    symlink(&first_symlink, &second_symlink).await.unwrap();
    symlink(&second_symlink, &first_symlink).await.unwrap();

    // Both symlinks loop on each other, so stating either one should produce
    // a file system loop error. This produces a `std::io::ErrorKind::FilesystemLoop`
    // error, but that error is gated behind io_error_more feature, and we can't be
    // sure if that name will ever be changed, so preferred using libc::ELOOP instead
    let filesystem_loop_result = try_exists(first_symlink).await;
    assert_eq!(
        filesystem_loop_result
            .err()
            .unwrap()
            .raw_os_error()
            .unwrap(),
        libc::ELOOP
    );
    let filesystem_loop_result = try_exists(second_symlink).await;
    assert_eq!(
        filesystem_loop_result
            .err()
            .unwrap()
            .raw_os_error()
            .unwrap(),
        libc::ELOOP
    );
}

#[tokio::test]
async fn stat_path_name_too_long() {
    let dir = tempdir().unwrap();
    // if we stat a file whose name is above PATH_MAX (Linux is 4096 bytes, Windows 260 chars or 32767 chars if extended
    // path is permitted), we should receive an std::io::ErrorKind::InvalidFilename error
    let long_nonexistent_path = dir.path().join(vec!["a"; (PATH_MAX + 1) as usize].join(""));
    let name_too_long_result = try_exists(long_nonexistent_path).await;
    assert_eq!(
        name_too_long_result.err().unwrap().kind(),
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
            let fut = try_exists(path.clone());

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

fn create_large_temp_file() -> (NamedTempFile, PathBuf) {
    let mut tmp = NamedTempFile::new().unwrap();
    let buf = create_buf(5000);

    tmp.write_all(&buf).unwrap();
    let path = tmp.path().to_path_buf();

    (tmp, path)
}

fn create_small_temp_file() -> (NamedTempFile, PathBuf) {
    let mut tmp = NamedTempFile::new().unwrap();
    let buf = create_buf(20);

    tmp.write_all(&buf).unwrap();
    let path = tmp.path().to_path_buf();

    (tmp, path)
}

fn create_buf(length: usize) -> Vec<u8> {
    (0..length).map(|i| i as u8).collect()
}
