#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable))]

use tokio::runtime::Runtime;
use tokio::time::{self, Duration};

#[test]
fn num_workers() {
    let rt = basic();
    assert_eq!(1, rt.metrics().num_workers());

    let rt = threaded();
    assert_eq!(2, rt.metrics().num_workers());
}

#[test]
fn remote_schedule_count() {
    use std::thread;

    let rt = basic();
    let handle = rt.handle().clone();
    let task = thread::spawn(move || {
        handle.spawn(async {
            // DO nothing
        })
    }).join().unwrap();

    rt.block_on(task).unwrap();

    assert_eq!(1, rt.metrics().remote_schedule_count());

    let rt = threaded();
    let handle = rt.handle().clone();
    let task = thread::spawn(move || {
        handle.spawn(async {
            // DO nothing
        })
    }).join().unwrap();

    rt.block_on(task).unwrap();

    assert_eq!(1, rt.metrics().remote_schedule_count());    
}

#[test]
fn worker_park_count() {
    let rt = basic();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert_eq!(2, metrics.worker_park_count(0));

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert!(1 <= metrics.worker_park_count(0));    
    assert_eq!(1, metrics.worker_park_count(1));
}

#[test]
fn worker_noop_count() {
    let rt = basic();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert_eq!(1, metrics.worker_noop_count(0));

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert!(1 <= metrics.worker_noop_count(0));    
    assert_eq!(1, metrics.worker_noop_count(1));
}

fn basic() -> Runtime {
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