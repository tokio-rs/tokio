#![allow(unknown_lints, unexpected_cfgs)]
#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), target_has_atomic = "64"))]

use std::sync::mpsc;
use std::time::Duration;
use tokio::runtime::Runtime;

#[test]
fn num_workers() {
    let rt = current_thread();
    assert_eq!(1, rt.metrics().num_workers());

    let rt = threaded();
    assert_eq!(2, rt.metrics().num_workers());
}

#[test]
fn num_alive_tasks() {
    let rt = current_thread();
    let metrics = rt.metrics();
    assert_eq!(0, metrics.num_alive_tasks());
    rt.block_on(rt.spawn(async move {
        assert_eq!(1, metrics.num_alive_tasks());
    }))
    .unwrap();

    assert_eq!(0, rt.metrics().num_alive_tasks());

    let rt = threaded();
    let metrics = rt.metrics();
    assert_eq!(0, metrics.num_alive_tasks());
    rt.block_on(rt.spawn(async move {
        assert_eq!(1, metrics.num_alive_tasks());
    }))
    .unwrap();

    // try for 10 seconds to see if this eventually succeeds.
    // wake_join() is called before the task is released, so in multithreaded
    // code, this means we sometimes exit the block_on before the counter decrements.
    for _ in 0..100 {
        if rt.metrics().num_alive_tasks() == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(0, rt.metrics().num_alive_tasks());
}

#[test]
fn injection_queue_depth_current_thread() {
    use std::thread;

    let rt = current_thread();
    let handle = rt.handle().clone();
    let metrics = rt.metrics();

    thread::spawn(move || {
        handle.spawn(async {});
    })
    .join()
    .unwrap();

    assert_eq!(1, metrics.injection_queue_depth());
}

#[test]
fn injection_queue_depth_multi_thread() {
    let rt = threaded();
    let metrics = rt.metrics();

    'next_try: for _ in 0..10 {
        let (tx, rx) = mpsc::channel();

        // Dropping _blocking_tasks will cause the blocking tasks to finish.
        let _blocking_tasks: Vec<_> = (0..2)
            .map(|_| {
                let tx = tx.clone();
                let (task, barrier) = mpsc::channel::<()>();

                // Spawn a task per runtime worker to block it.
                rt.spawn(async move {
                    tx.send(()).unwrap();
                    barrier.recv().ok();
                });

                task
            })
            .collect();

        // Make sure both tasks are blocking the runtime so that we can
        // deterministically fill up the injection queue with pending tasks.
        //
        // If this times out, we deadlocked the runtime (which can happen and is
        // expected behaviour) and retry the test until we manage to either not
        // deadlock the runtime or exhaust all tries, the latter causing the
        // test to fail.
        for _ in 0..2 {
            if rx.recv_timeout(Duration::from_secs(1)).is_err() {
                continue 'next_try;
            }
        }

        for i in 0..10 {
            assert_eq!(i, metrics.injection_queue_depth());
            rt.spawn(async {});
        }

        return;
    }

    panic!("runtime deadlocked each try");
}

fn current_thread() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn threaded() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
