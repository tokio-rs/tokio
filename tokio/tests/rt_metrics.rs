#![allow(unknown_lints, unexpected_cfgs)]
#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), target_has_atomic = "64"))]

use std::sync::{Arc, Barrier};
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

    let barrier1 = Arc::new(Barrier::new(3));
    let barrier2 = Arc::new(Barrier::new(3));

    // Spawn a task per runtime worker to block it.
    for _ in 0..2 {
        let barrier1 = barrier1.clone();
        let barrier2 = barrier2.clone();
        rt.spawn(async move {
            barrier1.wait();
            barrier2.wait();
        });
    }

    barrier1.wait();

    for i in 0..10 {
        assert_eq!(i, metrics.injection_queue_depth());
        rt.spawn(async {});
    }

    barrier2.wait();
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
