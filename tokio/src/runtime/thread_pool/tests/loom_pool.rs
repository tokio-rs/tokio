use crate::runtime::tests::loom_oneshot as oneshot;
use crate::runtime::{self, Runtime};
use crate::spawn;

use loom::sync::atomic::{AtomicBool, AtomicUsize};
use loom::sync::{Arc, Mutex};

use std::future::Future;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

#[test]
fn pool_multi_spawn() {
    loom::model(|| {
        let pool = mk_pool(2);
        let c1 = Arc::new(AtomicUsize::new(0));

        let (tx, rx) = oneshot::channel();
        let tx1 = Arc::new(Mutex::new(Some(tx)));

        // Spawn a task
        let c2 = c1.clone();
        let tx2 = tx1.clone();
        pool.spawn(async move {
            spawn(async move {
                if 1 == c1.fetch_add(1, Relaxed) {
                    tx1.lock().unwrap().take().unwrap().send(());
                }
            });
        });

        // Spawn a second task
        pool.spawn(async move {
            spawn(async move {
                if 1 == c2.fetch_add(1, Relaxed) {
                    tx2.lock().unwrap().take().unwrap().send(());
                }
            });
        });

        rx.recv();
    });
}

#[test]
fn only_blocking() {
    loom::model(|| {
        let pool = mk_pool(1);
        let (block_tx, block_rx) = oneshot::channel();

        pool.spawn(async move {
            crate::task::block_in_place(move || {
                block_tx.send(());
            })
        });

        block_rx.recv();
        drop(pool);
    });
}

#[test]
fn blocking_and_regular() {
    const NUM: usize = 3;
    loom::model(|| {
        let pool = mk_pool(1);
        let cnt = Arc::new(AtomicUsize::new(0));

        let (block_tx, block_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let done_tx = Arc::new(Mutex::new(Some(done_tx)));

        pool.spawn(async move {
            crate::task::block_in_place(move || {
                block_tx.send(());
            })
        });

        for _ in 0..NUM {
            let cnt = cnt.clone();
            let done_tx = done_tx.clone();

            pool.spawn(async move {
                if NUM == cnt.fetch_add(1, Relaxed) + 1 {
                    done_tx.lock().unwrap().take().unwrap().send(());
                }
            });
        }

        done_rx.recv();
        block_rx.recv();

        drop(pool);
    });
}

#[test]
fn pool_multi_notify() {
    loom::model(|| {
        let pool = mk_pool(2);

        let c1 = Arc::new(AtomicUsize::new(0));

        let (done_tx, done_rx) = oneshot::channel();
        let done_tx1 = Arc::new(Mutex::new(Some(done_tx)));

        // Spawn a task
        let c2 = c1.clone();
        let done_tx2 = done_tx1.clone();
        pool.spawn(async move {
            gated().await;
            gated().await;

            if 1 == c1.fetch_add(1, Relaxed) {
                done_tx1.lock().unwrap().take().unwrap().send(());
            }
        });

        // Spawn a second task
        pool.spawn(async move {
            gated().await;
            gated().await;

            if 1 == c2.fetch_add(1, Relaxed) {
                done_tx2.lock().unwrap().take().unwrap().send(());
            }
        });

        done_rx.recv();
    });
}

#[test]
fn pool_shutdown() {
    loom::model(|| {
        let pool = mk_pool(2);

        pool.spawn(async move {
            gated2(true).await;
        });

        pool.spawn(async move {
            gated2(false).await;
        });

        drop(pool);
    });
}

#[test]
fn complete_block_on_under_load() {
    use futures::FutureExt;

    loom::model(|| {
        let mut pool = mk_pool(2);

        pool.block_on({
            futures::future::lazy(|_| ()).then(|_| {
                // Spin hard
                crate::spawn(async {
                    for _ in 0..2 {
                        yield_once().await;
                    }
                });

                gated2(true)
            })
        });
    });
}

#[test]
fn shutdown_with_notification() {
    use crate::stream::StreamExt;
    use crate::sync::{mpsc, oneshot};

    loom::model(|| {
        let rt = mk_pool(2);
        let (done_tx, done_rx) = oneshot::channel::<()>();

        rt.spawn(async move {
            let (mut tx, mut rx) = mpsc::channel::<()>(10);

            crate::spawn(async move {
                crate::task::spawn_blocking(move || {
                    let _ = tx.try_send(());
                });

                let _ = done_rx.await;
            });

            while let Some(_) = rx.next().await {}

            let _ = done_tx.send(());
        });
    });
}

fn mk_pool(num_threads: usize) -> Runtime {
    runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(num_threads)
        .build()
        .unwrap()
}

use futures::future::poll_fn;
use std::task::Poll;
async fn yield_once() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            loom::thread::yield_now();
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await
}

fn gated() -> impl Future<Output = &'static str> {
    gated2(false)
}

fn gated2(thread: bool) -> impl Future<Output = &'static str> {
    use loom::thread;
    use std::sync::Arc;

    let gate = Arc::new(AtomicBool::new(false));
    let mut fired = false;

    poll_fn(move |cx| {
        if !fired {
            let gate = gate.clone();
            let waker = cx.waker().clone();

            if thread {
                thread::spawn(move || {
                    gate.store(true, Release);
                    waker.wake_by_ref();
                });
            } else {
                spawn(async move {
                    gate.store(true, Release);
                    waker.wake_by_ref();
                });
            }

            fired = true;

            return Poll::Pending;
        }

        if gate.load(Acquire) {
            Poll::Ready("hello world")
        } else {
            Poll::Pending
        }
    })
}
