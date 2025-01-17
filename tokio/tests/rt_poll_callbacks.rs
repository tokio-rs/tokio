use std::{
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use tokio::time::sleep;

//#[cfg(tokio_unstable)]
#[test]
fn callbacks_fire() {
    let poll_start_counter = Arc::new(AtomicUsize::new(0));
    let poll_stop_counter = Arc::new(AtomicUsize::new(0));
    let poll_start = poll_start_counter.clone();
    let poll_stop = poll_stop_counter.clone();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .on_before_task_poll(move |_meta| {
            poll_start_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .on_after_task_poll(move |_meta| {
            poll_stop_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .build()
        .unwrap();
    let task = rt.spawn(async {
        sleep(Duration::from_millis(100)).await;
        sleep(Duration::from_millis(100)).await;
        sleep(Duration::from_millis(100)).await;
    });
    rt.block_on(task);
    assert_eq!(poll_start.load(std::sync::atomic::Ordering::Relaxed), 4);
    assert_eq!(poll_stop.load(std::sync::atomic::Ordering::Relaxed), 4);
}
