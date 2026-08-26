#![warn(rust_2018_idioms)]
#![cfg(all(
    target_os = "emscripten",
    feature = "rt-multi-thread",
    feature = "macros"
))]

//! Multi-thread runtime tests for `wasm32-unknown-emscripten` built with
//! pthreads (`-pthread` and `-sPROXY_TO_PTHREAD`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

#[test]
fn block_on_multi_thread() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()
        .unwrap();

    let out = rt.block_on(async {
        let jh = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            "hello"
        });
        jh.await.unwrap()
    });
    assert_eq!(out, "hello");
}

#[test]
fn spawn_blocking_runs_in_parallel() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .unwrap();

    // Both closures must be running concurrently for the barrier to release.
    let barrier = Arc::new(Barrier::new(2));
    rt.block_on(async {
        let a = tokio::task::spawn_blocking({
            let barrier = barrier.clone();
            move || {
                barrier.wait();
            }
        });
        let b = tokio::task::spawn_blocking(move || {
            barrier.wait();
        });
        a.await.unwrap();
        b.await.unwrap();
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn macro_multi_thread() {
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(tokio::spawn(async {
            tokio::task::yield_now().await;
            COUNT.fetch_add(1, Ordering::Relaxed);
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(COUNT.load(Ordering::Relaxed), 8);
}
