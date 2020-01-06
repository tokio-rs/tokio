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

#[test]
fn wake_while_rt_is_dropping() {
    use tokio::task;

    struct OnDrop<F: FnMut()>(F);

    impl<F: FnMut()> Drop for OnDrop<F> {
        fn drop(&mut self) {
            (self.0)()
        }
    }

    let (tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();
    let (tx3, rx3) = oneshot::channel();

    let mut rt = rt();

    let h1 = rt.handle().clone();

    rt.handle().spawn(async move {
        // Ensure a waker gets stored in oneshot 1.
        let _ = rx1.await;
        tx3.send(()).unwrap();
    });

    rt.handle().spawn(async move {
        // When this task is dropped, we'll be "closing remotes".
        // We spawn a new task that owns the `tx1`, to move its Drop
        // out of here.
        //
        // Importantly, the oneshot 1 has a waker already stored, so
        // the eventual drop here will try to re-schedule again.
        let mut opt_tx1 = Some(tx1);
        let _d = OnDrop(move || {
            let tx1 = opt_tx1.take().unwrap();
            h1.spawn(async move {
                tx1.send(()).unwrap();
            });
        });
        let _ = rx2.await;
    });

    rt.handle().spawn(async move {
        let _ = rx3.await;
        // We'll never get here, but once task 3 drops, this will
        // force task 2 to re-schedule since it's waiting on oneshot 2.
        tx2.send(()).unwrap();
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
