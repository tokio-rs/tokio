//! Uring mkdir operations tests.

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
use std::pin::pin;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
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
        let workdir = tempfile::tempdir().unwrap();

        rt.spawn(async move {
            // spawning a bunch of uring operations.
            for i in 0..usize::MAX {
                let child = workdir.path().join(format!("{i}"));
                tokio::spawn(async move {
                    let mut fut = pin!(tokio::fs::create_dir(&child));

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
fn create_many_directories() {
    fn run(rt: Runtime) {
        let workdir = tempfile::tempdir().unwrap();

        rt.block_on(async move {
            const N_CHILDREN: usize = 10_000;

            let tracker = TaskTracker::new();

            for i in 0..N_CHILDREN {
                let child = workdir.path().join(format!("{i}"));
                tracker.spawn(async move {
                    tokio::fs::create_dir(&child).await.unwrap();
                });
            }
            tracker.close();
            tracker.wait().await;

            let mut dir_iter = tokio::fs::read_dir(workdir.path()).await.unwrap();
            let mut child_count = 0;
            while dir_iter.next_entry().await.unwrap().is_some() {
                child_count += 1;
            }
            assert_eq!(child_count, N_CHILDREN);
        });
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn create_dir_all_edge_cases() {
    let workdir = tempfile::tempdir().unwrap();
    let workdir_path = workdir.path();

    tokio::fs::create_dir_all(workdir.path()).await.unwrap();

    let nested_path = workdir_path.join("foo").join("bar");
    tokio::fs::create_dir_all(&nested_path).await.unwrap();
    assert!(nested_path.is_dir());
    tokio::fs::create_dir_all(nested_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::create_dir_all(&nested_path).await.unwrap();

    let slash_trailing = workdir_path.join("./baz/qux//");
    tokio::fs::create_dir_all(&slash_trailing).await.unwrap();
    assert!(slash_trailing.is_dir());
}

#[tokio::test]
async fn cancel_op_future() {
    let workdir = tempfile::tempdir().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let child = workdir.path().join("child");
            let fut = tokio::fs::create_dir(&child);

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
