//! Uring symlink operations tests.

#![cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]

use futures::FutureExt;
use std::future::poll_fn;
use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinSet;
use tokio_test::assert_pending;
use tokio_util::task::TaskTracker;

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
    fn run(rt: Runtime) {
        let (done_tx, done_rx) = mpsc::channel();
        let (workdir, target) = create_tmp_dir();

        rt.spawn(async move {
            // spawning a bunch of uring operations.
            for i in 0..usize::MAX {
                let link = workdir.path().join(format!("{i}"));
                let target = target.clone();
                tokio::spawn(async move {
                    let mut fut = pin!(tokio::fs::symlink(target, &link));

                    poll_fn(|cx| {
                        assert_pending!(fut.as_mut().poll(cx));
                        Poll::<()>::Pending
                    })
                    .await;

                    fut.await.unwrap();
                });

                // Avoid busy looping.
                tokio::task::yield_now().await;
            }
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
fn symlink_many_files() {
    fn run(rt: Runtime) {
        let (workdir, target) = create_tmp_dir();

        rt.block_on(async move {
            const N_LINKS: usize = 10_000;

            let tracker = TaskTracker::new();

            for i in 0..N_LINKS {
                let target = target.clone();
                let link = workdir.path().join(format!("{i}"));
                tracker.spawn(async move {
                    tokio::fs::symlink(&target, &link).await.unwrap();
                });
            }
            tracker.close();
            tracker.wait().await;

            let mut resolve_tasks = JoinSet::new();
            for i in 0..N_LINKS {
                let link = workdir.path().join(format!("{i}"));
                resolve_tasks.spawn(async move { tokio::fs::read_link(&link).await.unwrap() });
            }

            while let Some(resolve_result) = resolve_tasks.join_next().await {
                assert_eq!(&resolve_result.unwrap(), &target);
            }
        });
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn cancel_op_future() {
    let (workdir, target) = create_tmp_dir();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let link = workdir.path().join("link");
            let fut = tokio::fs::symlink(&target, &link);

            // If io_uring is enabled (and not falling back to the thread pool),
            // the first poll should return Pending.
            let _pending = pin!(fut).poll_unpin(cx);

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

fn create_tmp_dir() -> (TempDir, PathBuf) {
    let workdir = tempfile::tempdir().unwrap();
    let target = workdir.path().join("target");
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
        .unwrap();

    (workdir, target)
}
