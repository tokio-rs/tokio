//! Uring file operations tests.

#![cfg(all(tokio_uring, feature = "rt", feature = "fs", target_os = "linux"))]

use futures::future::FutureExt;
use std::sync::mpsc;
use std::task::Poll;
use std::{future::poll_fn, path::PathBuf};
use tempfile::NamedTempFile;
use tokio::{
    fs::OpenOptions,
    runtime::{Builder, Runtime},
    task::JoinSet,
};

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
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        rt.spawn(async {
            let (_tmp, path) = create_tmp_files(1);
            let path = path[0].clone();

            // spawning a bunch of uring operations.
            loop {
                let path = path.clone();
                tokio::spawn(async move {
                    let mut opt = OpenOptions::new();
                    opt.read(true);
                    opt.open(&path).await.unwrap();
                });

                // Avoid busy looping.
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });

        std::thread::spawn(move || {
            let rt: Runtime = rx.recv().unwrap();
            rt.shutdown_background();
            done_tx.send(()).unwrap();
        });

        tx.send(rt).unwrap();
        done_rx.recv().unwrap();
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[test]
fn open_many_files() {
    fn run(rt: Runtime) {
        const NUM_FILES: usize = 512;

        let (_tmp_files, paths): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(NUM_FILES);

        rt.block_on(async move {
            let mut set = JoinSet::new();

            for i in 0..10_000 {
                let path = paths.get(i % NUM_FILES).unwrap().clone();
                set.spawn(async move {
                    let _file = OpenOptions::new().read(true).open(path).await.unwrap();
                });
            }
            while let Some(Ok(_)) = set.join_next().await {}
        });
    }

    for rt in rt_combinations() {
        run(rt());
    }
}

#[tokio::test]
async fn cancel_op_future() {
    let (_tmp_file, path): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(1);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = tokio::spawn(async move {
        poll_fn(|cx| {
            let opt = {
                let mut opt = tokio::fs::OpenOptions::new();
                opt.read(true);
                opt
            };

            let fut = opt.open(&path[0]);
            let res = Box::pin(fut).poll_unpin(cx);

            // First poll should be pending.
            assert!(res.is_pending(), "Expected the open to be pending");

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

fn create_tmp_files(num_files: usize) -> (Vec<NamedTempFile>, Vec<PathBuf>) {
    let mut files = Vec::with_capacity(num_files);
    for _ in 0..num_files {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        files.push((tmp, path));
    }

    files.into_iter().unzip()
}
