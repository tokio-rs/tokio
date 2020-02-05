#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};
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
fn no_extra_poll() {
    use std::pin::Pin;
    use std::sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    };
    use std::task::{Context, Poll};
    use tokio::stream::{Stream, StreamExt};

    struct TrackPolls<S> {
        npolls: Arc<AtomicUsize>,
        s: S,
    }

    impl<S> Stream for TrackPolls<S>
    where
        S: Stream,
    {
        type Item = S::Item;
        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            // safety: we do not move s
            let this = unsafe { self.get_unchecked_mut() };
            this.npolls.fetch_add(1, SeqCst);
            // safety: we are pinned, and so is s
            unsafe { Pin::new_unchecked(&mut this.s) }.poll_next(cx)
        }
    }

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = TrackPolls {
        npolls: Arc::new(AtomicUsize::new(0)),
        s: rx,
    };
    let npolls = Arc::clone(&rx.npolls);

    let mut rt = rt();

    rt.spawn(async move { while let Some(_) = rx.next().await {} });
    rt.block_on(async {
        tokio::task::yield_now().await;
    });

    // should have been polled exactly once: the initial poll
    assert_eq!(npolls.load(SeqCst), 1);

    tx.send(()).unwrap();
    rt.block_on(async {
        tokio::task::yield_now().await;
    });

    // should have been polled twice more: once to yield Some(), then once to yield Pending
    assert_eq!(npolls.load(SeqCst), 1 + 2);

    drop(tx);
    rt.block_on(async {
        tokio::task::yield_now().await;
    });

    // should have been polled once more: to yield None
    assert_eq!(npolls.load(SeqCst), 1 + 2 + 1);
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
