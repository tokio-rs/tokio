#![warn(rust_2018_idioms)]
#![cfg(all(
    feature = "full",
    tokio_unstable,
    not(target_os = "wasi"),
    target_has_atomic = "64"
))]

use tokio::runtime::{self, Runtime};

#[test]
fn worker_id_multi_thread() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let id = tokio::task::spawn(async { runtime::worker_id() })
            .await
            .unwrap();
        let num_workers = rt.metrics().num_workers();
        let id = id.expect("should be Some on worker thread");
        assert!(
            id < num_workers,
            "worker_id {id} >= num_workers {num_workers}"
        );
    });
}

#[test]
fn worker_id_current_thread() {
    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let id = runtime::worker_id();
        assert_eq!(id, Some(0));
    });
}

#[test]
fn worker_id_outside_runtime() {
    assert_eq!(runtime::worker_id(), None);
}

#[test]
fn worker_id_matches_metrics_worker_thread_id() {
    let rt = runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let metrics = rt.metrics();

    rt.block_on(async {
        // Spawn a task and verify the worker_id matches the metrics index
        tokio::task::spawn(async move {
            let wid = runtime::worker_id().expect("should be on worker thread");
            let current_thread = std::thread::current().id();
            let metrics_thread = metrics.worker_thread_id(wid);
            assert_eq!(
                metrics_thread,
                Some(current_thread),
                "worker_id() returned {wid} but metrics.worker_thread_id({wid}) \
                 does not match current thread"
            );
        })
        .await
        .unwrap();
    });
}

#[test]
fn worker_id_from_spawn_blocking() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let id = tokio::task::spawn_blocking(runtime::worker_id)
            .await
            .unwrap();
        assert_eq!(id, None, "spawn_blocking should not be on a worker thread");
    });
}

#[test]
fn worker_id_block_on_multi_thread() {
    let rt = Runtime::new().unwrap();
    // block_on runs on the calling thread, not a worker thread
    let id = rt.block_on(async { runtime::worker_id() });
    assert_eq!(
        id, None,
        "block_on thread is not a worker thread on multi-thread runtime"
    );
}

#[tokio::main(flavor = "current_thread")]
#[test]
async fn worker_id_tokio_main_current_thread() {
    // current_thread block_on runs on the worker, so this is Some(0)
    assert_eq!(runtime::worker_id(), Some(0));
}
