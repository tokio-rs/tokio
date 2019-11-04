use crate::executor::loom::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use crate::executor::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::executor::loom::sync::{Arc, Mutex};
use crate::executor::park::{Park, Unpark};
use crate::executor::tests::loom_oneshot as oneshot;
use crate::executor::thread_pool::{self, Builder};
use crate::spawn;

use loom::sync::Notify;

use std::future::Future;
use std::time::Duration;

#[test]
fn pool_multi_spawn() {
    loom::model(|| {
        let pool = Builder::new()
            .build(|_| LoomPark::new());

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
        let mut pool = Builder::new().num_threads(1).build(|_| LoomPark::new());
        let (block_tx, block_rx) = oneshot::channel();

        pool.spawn(async move {
            thread_pool::blocking(move || {
                block_tx.send(());
            })
        });

        block_rx.recv();
        pool.shutdown_now();
    });
}

#[test]
fn blocking_and_regular() {
    const NUM: usize = 3;
    loom::model(|| {
        let mut pool = Builder::new().num_threads(1).build(|_| LoomPark::new());
        let cnt = Arc::new(AtomicUsize::new(0));

        let (block_tx, block_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let done_tx = Arc::new(Mutex::new(Some(done_tx)));

        pool.spawn(async move {
            thread_pool::blocking(move || {
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

        pool.shutdown_now();
    });
}

#[test]
fn pool_multi_notify() {
    loom::model(|| {
        let pool = Builder::new().build(|_| LoomPark::new());

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
        let pool = Builder::new()
            .build(|_| LoomPark::new());

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
    loom::model(|| {
        let pool = Builder::new()
            .build(|_| LoomPark::new());

        pool.block_on(async {
            // Spin hard
            crate::spawn(async {
                for _ in 0..2 {
                    yield_once().await;
                }
            });

            gated2(true).await
        });
    });
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
    use crate::executor::loom::thread;
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

struct LoomPark {
    notify: Arc<Notify>,
}

struct LoomUnpark {
    notify: Arc<Notify>,
}

impl LoomPark {
    fn new() -> LoomPark {
        LoomPark {
            notify: Arc::new(Notify::new()),
        }
    }
}

impl Park for LoomPark {
    type Unpark = LoomUnpark;

    type Error = ();

    fn unpark(&self) -> LoomUnpark {
        let notify = self.notify.clone();
        LoomUnpark { notify }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.notify.wait();
        Ok(())
    }

    fn park_timeout(&mut self, _duration: Duration) -> Result<(), Self::Error> {
        self.notify.wait();
        Ok(())
    }
}

impl Unpark for LoomUnpark {
    fn unpark(&self) {
        self.notify.notify();
    }
}
