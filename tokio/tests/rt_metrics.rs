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
fn global_queue_depth_current_thread() {
    use std::thread;

    let rt = current_thread();
    let handle = rt.handle().clone();
    let metrics = rt.metrics();

    thread::spawn(move || {
        handle.spawn(async {});
    })
    .join()
    .unwrap();

    assert_eq!(1, metrics.global_queue_depth());
}

#[test]
fn global_queue_depth_multi_thread() {
    for _ in 0..10 {
        let rt = threaded();
        let metrics = rt.metrics();

        if let Ok(_blocking_tasks) = try_block_threaded(&rt) {
            for i in 0..10 {
                assert_eq!(i, metrics.global_queue_depth());
                rt.spawn(async {});
            }

            return;
        }
    }

    panic!("exhausted every try to block the runtime");
}

fn try_block_threaded(rt: &Runtime) -> Result<Vec<mpsc::Sender<()>>, mpsc::RecvTimeoutError> {
    let (tx, rx) = mpsc::channel();

    let blocking_tasks = (0..rt.metrics().num_workers())
        .map(|_| {
            let tx = tx.clone();
            let (task, barrier) = mpsc::channel();

            // Spawn a task per runtime worker to block it.
            rt.spawn(async move {
                tx.send(()).ok();
                barrier.recv().ok();
            });

            task
        })
        .collect();

    // Make sure the previously spawned tasks are blocking the runtime by
    // receiving a message from each blocking task.
    //
    // If this times out we were unsuccessful in blocking the runtime and hit
    // a deadlock instead (which might happen and is expected behaviour).
    for _ in 0..rt.metrics().num_workers() {
        rx.recv_timeout(Duration::from_secs(1))?;
    }

    // Return senders of the mpsc channels used for blocking the runtime as a
    // surrogate handle for the tasks. Sending a message or dropping the senders
    // will unblock the runtime.
    Ok(blocking_tasks)
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
