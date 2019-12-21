#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio_test::{assert_err, assert_ok};

use std::thread;
use std::time::Duration;

#[test]
fn spawned_task_does_not_progress_without_block_on() {
    let (tx, mut rx) = oneshot::channel();

    let mut rt = rt();

    rt.spawn(async move {
        assert_ok!(tx.send("hello"));
    });

    thread::sleep(Duration::from_millis(50));

    assert_err!(rx.try_recv());

    let out = rt.block_on(async { assert_ok!(rx.await) });

    assert_eq!(out, "hello");
}

#[test]
fn acquire_mutex_in_drop() {
    use futures::future::pending;
    use tokio::task;

    let (tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();

    let mut rt = rt();

    rt.spawn(async move {
        let _ = rx2.await;
        unreachable!();
    });

    rt.spawn(async move {
        let _ = rx1.await;
        tx2.send(()).unwrap();
        unreachable!();
    });

    // Spawn a task that will never notify
    rt.spawn(async move {
        pending::<()>().await;
        tx1.send(()).unwrap();
    });

    // Tick the loop
    rt.block_on(async {
        task::yield_now().await;
    });

    // Drop the rt
    drop(rt);
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()
        .unwrap()
}
