#![feature(test)]

extern crate test;

use tokio::runtime::Builder;
use tokio::sync::oneshot;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{mpsc, Arc};
use std::task::{Context, Poll};

struct Backoff(usize);

impl Future for Backoff {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 == 0 {
            Poll::Ready(())
        } else {
            self.0 -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[bench]
fn spawn_many(b: &mut test::Bencher) {
    const NUM_SPAWN: usize = 10_000;

    let rt = Builder::new().threaded_scheduler().build().unwrap();

    let (tx, rx) = mpsc::sync_channel(1000);
    let rem = Arc::new(AtomicUsize::new(0));

    b.iter(|| {
        rem.store(NUM_SPAWN, Relaxed);

        for _ in 0..NUM_SPAWN {
            let tx = tx.clone();
            let rem = rem.clone();

            rt.spawn(async move {
                if 1 == rem.fetch_sub(1, Relaxed) {
                    tx.send(()).unwrap();
                }
            });
        }

        let _ = rx.recv().unwrap();
    });
}

#[bench]
fn yield_many(b: &mut test::Bencher) {
    const NUM_YIELD: usize = 1_000;
    const TASKS_PER_CPU: usize = 50;

    let rt = Builder::new().threaded_scheduler().build().unwrap();

    let tasks = TASKS_PER_CPU * num_cpus::get_physical();
    let (tx, rx) = mpsc::sync_channel(tasks);

    b.iter(move || {
        for _ in 0..tasks {
            let tx = tx.clone();

            rt.spawn(async move {
                let backoff = Backoff(NUM_YIELD);
                backoff.await;
                tx.send(()).unwrap();
            });
        }

        for _ in 0..tasks {
            let _ = rx.recv().unwrap();
        }
    });
}

#[bench]
fn ping_pong(b: &mut test::Bencher) {
    const NUM_PINGS: usize = 1_000;

    let rt = Builder::new().threaded_scheduler().build().unwrap();

    let (done_tx, done_rx) = mpsc::sync_channel(1000);
    let rem = Arc::new(AtomicUsize::new(0));

    b.iter(|| {
        let done_tx = done_tx.clone();
        let rem = rem.clone();
        rem.store(NUM_PINGS, Relaxed);

        rt.spawn(async move {
            for _ in 0..NUM_PINGS {
                let rem = rem.clone();
                let done_tx = done_tx.clone();

                tokio::spawn(async move {
                    let (tx1, rx1) = oneshot::channel();
                    let (tx2, rx2) = oneshot::channel();

                    tokio::spawn(async move {
                        rx1.await.unwrap();
                        tx2.send(()).unwrap();
                    });

                    tx1.send(()).unwrap();
                    rx2.await.unwrap();

                    if 1 == rem.fetch_sub(1, Relaxed) {
                        done_tx.send(()).unwrap();
                    }
                });
            }
        });

        done_rx.recv().unwrap();
    });
}

#[bench]
fn chained_spawn(b: &mut test::Bencher) {
    const ITER: usize = 1_000;

    let rt = Builder::new().threaded_scheduler().build().unwrap();

    fn iter(done_tx: mpsc::SyncSender<()>, n: usize) {
        if n == 0 {
            done_tx.send(()).unwrap();
        } else {
            tokio::spawn(async move {
                iter(done_tx, n - 1);
            });
        }
    }

    let (done_tx, done_rx) = mpsc::sync_channel(1000);

    b.iter(move || {
        let done_tx = done_tx.clone();
        rt.spawn(async move {
            iter(done_tx, ITER);
        });

        done_rx.recv().unwrap();
    });
}
