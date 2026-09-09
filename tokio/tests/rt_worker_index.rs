#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"),))]

use std::sync::Arc;
use std::thread;

use tokio::runtime::{self, Runtime};

#[test]
fn worker_index_current_thread() {
    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let index = runtime::worker_index();
        assert_eq!(index, Some(0));
    });
}

#[test]
fn worker_index_handle_block_on_current_thread() {
    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let index = rt.handle().block_on(async { runtime::worker_index() });
    assert_eq!(index, None);
}

#[test]
fn worker_index_current_thread_concurrent_block_on() {
    let rt = Arc::new(
        runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();

    let owner_rt = rt.clone();
    let owner = thread::spawn(move || {
        owner_rt.block_on(async {
            entered_tx.send(()).unwrap();
            release_rx.await.unwrap();
            runtime::worker_index()
        })
    });

    entered_rx.recv().unwrap();

    let other_rt = rt.clone();
    let other = thread::spawn(move || other_rt.block_on(async { runtime::worker_index() }));

    assert_eq!(other.join().unwrap(), None);
    release_tx.send(()).unwrap();
    assert_eq!(owner.join().unwrap(), Some(0));
}

#[test]
fn worker_index_local_runtime() {
    let rt = runtime::LocalRuntime::new().unwrap();
    rt.block_on(async {
        let index = runtime::worker_index();
        assert_eq!(index, Some(0));
    });
}

#[test]
fn worker_index_outside_runtime() {
    assert_eq!(runtime::worker_index(), None);
}

#[cfg(all(tokio_unstable, target_has_atomic = "64"))]
#[test]
fn worker_index_matches_metrics_worker_thread_id() {
    let rt = runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let metrics = rt.metrics();

    rt.block_on(async {
        // Spawn a task and verify the worker_index matches the metrics index
        tokio::task::spawn(async move {
            let index = runtime::worker_index().expect("should be on worker thread");
            let current_thread = std::thread::current().id();
            let metrics_thread = metrics.worker_thread_id(index);
            assert_eq!(
                metrics_thread,
                Some(current_thread),
                "worker_index() returned {index} but metrics.worker_thread_id({index}) \
                 does not match current thread"
            );
        })
        .await
        .unwrap();
    });
}

#[test]
fn worker_index_from_spawn_blocking() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let index = tokio::task::spawn_blocking(runtime::worker_index)
            .await
            .unwrap();
        assert_eq!(
            index, None,
            "spawn_blocking should not be on a worker thread"
        );
    });
}

#[test]
fn worker_index_block_on_multi_thread() {
    let rt = Runtime::new().unwrap();
    // block_on runs on the calling thread, not a worker thread
    let index = rt.block_on(async { runtime::worker_index() });
    assert_eq!(
        index, None,
        "block_on thread is not a worker thread on multi-thread runtime"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn worker_index_tokio_test_current_thread() {
    assert_eq!(runtime::worker_index(), Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn worker_index_tokio_test_multi_thread() {
    assert_eq!(runtime::worker_index(), None);
}
