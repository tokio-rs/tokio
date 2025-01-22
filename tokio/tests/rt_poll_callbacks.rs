#![allow(unknown_lints, unexpected_cfgs)]
use std::sync::{atomic::AtomicUsize, Arc};

use tokio::task::yield_now;

#[cfg(tokio_unstable)]
#[cfg(not(target_os = "wasi"))]
#[test]
fn callbacks_fire_multi_thread() {
    let poll_start_counter = Arc::new(AtomicUsize::new(0));
    let poll_stop_counter = Arc::new(AtomicUsize::new(0));
    let poll_start = poll_start_counter.clone();
    let poll_stop = poll_stop_counter.clone();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .on_before_task_poll(move |_meta| {
            poll_start_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .on_after_task_poll(move |_meta| {
            poll_stop_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .build()
        .unwrap();
    let task = rt.spawn(async {
        yield_now().await;
        yield_now().await;
        yield_now().await;
    });
    let _ = rt.block_on(task);
    assert_eq!(poll_start.load(std::sync::atomic::Ordering::SeqCst), 4);
    assert_eq!(poll_stop.load(std::sync::atomic::Ordering::SeqCst), 4);
}
