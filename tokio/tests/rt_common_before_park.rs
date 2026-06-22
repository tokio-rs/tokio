#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(not(target_os = "wasi"))] // Wasi doesn't support threads

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::sync::Notify;

#[test]
fn before_park_wakes_block_on_task() {
    let notify = Arc::new(Notify::new());
    let notify2 = notify.clone();
    let woken = Arc::new(AtomicBool::new(false));
    let woken2 = woken.clone();

    let rt = Builder::new_current_thread()
        .enable_all()
        .on_thread_park(move || {
            // Only wake once to avoid busy loop if something goes wrong,
            // though in this test we expect it to unpark immediately.
            if !woken2.swap(true, Ordering::SeqCst) {
                notify2.notify_one();
            }
        })
        .build()
        .unwrap();

    rt.block_on(async {
        // This will block until `notify` is notified.
        // `before_park` should run when the runtime is about to park.
        // It will notify `notify`, which should wake this task.
        // The runtime should then see the task is woken and NOT park.
        notify.notified().await;
    });

    assert!(woken.load(Ordering::SeqCst));
}

#[test]
fn before_park_spawns_task() {
    let notify = Arc::new(Notify::new());
    let notify2 = notify.clone();
    let woken = Arc::new(AtomicBool::new(false));
    let woken2 = woken.clone();

    let rt = Builder::new_current_thread()
        .enable_all()
        .on_thread_park(move || {
            if !woken2.swap(true, Ordering::SeqCst) {
                let notify = notify2.clone();
                tokio::spawn(async move {
                    notify.notify_one();
                });
            }
        })
        .build()
        .unwrap();

    rt.block_on(async {
        // This will block until `notify` is notified.
        // `before_park` should run when the runtime is about to park.
        // It will spawn a task that notifies `notify`.
        // The runtime should see the new task and NOT park.
        // If it parks, it will deadlock.
        notify.notified().await;
    });

    assert!(woken.load(Ordering::SeqCst));
}

#[test]
fn wake_from_other_thread_block_on() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    let handle = rt.handle().clone();
    let notify = Arc::new(Notify::new());
    let notify2 = notify.clone();

    let th = std::thread::spawn(move || {
        // Give the main thread time to park
        std::thread::sleep(std::time::Duration::from_millis(5));
        handle.block_on(async move {
            notify2.notify_one();
        });
    });

    rt.block_on(async {
        notify.notified().await;
    });

    th.join().unwrap();
}

// Regression test for #8212: a current-thread runtime driven by repeated short
// `block_on` calls, where `on_thread_park` wakes the `block_on` future, must
// still drive the time driver so a spawned timer keeps making progress. Before
// the fix, `before_park` setting the `woken` flag caused `park` to skip the
// driver entirely, so the timer never fired.
#[test]
fn before_park_does_not_stall_spawned_timer() {
    let notify = Arc::new(Notify::new());
    let task_done = Arc::new(AtomicBool::new(false));

    let rt = Builder::new_current_thread()
        .enable_all()
        .on_thread_park({
            let notify = notify.clone();
            move || notify.notify_waiters()
        })
        .build()
        .unwrap();

    rt.spawn({
        let task_done = task_done.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            task_done.store(true, Ordering::SeqCst);
        }
    });

    // Let the spawned task register its timer before we start driving.
    std::thread::sleep(Duration::from_millis(10));

    // Drive the runtime in short bursts, the way an external event loop would.
    for _ in 0..60 {
        if task_done.load(Ordering::SeqCst) {
            break;
        }
        rt.block_on(async {
            let notify = notify.clone();
            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        });
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        task_done.load(Ordering::SeqCst),
        "spawned task's timer never fired (issue #8212)"
    );
}
