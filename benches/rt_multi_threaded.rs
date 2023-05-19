//! Benchmark implementation details of the threaded scheduler. These benches are
//! intended to be used as a form of regression testing and not as a general
//! purpose benchmark demonstrating real-world performance.

use tokio::runtime::{self, Runtime};
use tokio::sync::oneshot;

use bencher::{benchmark_group, benchmark_main, Bencher};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{mpsc, Arc};

const NUM_WORKERS: usize = 4;
const NUM_SPAWN: usize = 10_000;

fn spawn_many_local(b: &mut Bencher) {
    let rt = rt();

    let (tx, rx) = mpsc::sync_channel(1000);
    let rem = Arc::new(AtomicUsize::new(0));

    b.iter(|| {
        rem.store(NUM_SPAWN, Relaxed);

        rt.block_on(async {
            for _ in 0..NUM_SPAWN {
                let tx = tx.clone();
                let rem = rem.clone();

                tokio::spawn(async move {
                    if 1 == rem.fetch_sub(1, Relaxed) {
                        tx.send(()).unwrap();
                    }
                });
            }

            let _ = rx.recv().unwrap();
        });
    });
}

fn spawn_many_remote_idle(b: &mut Bencher) {
    let rt = rt();

    let mut handles = Vec::with_capacity(NUM_SPAWN);

    b.iter(|| {
        for _ in 0..NUM_SPAWN {
            handles.push(rt.spawn(async {}));
        }

        rt.block_on(async {
            for handle in handles.drain(..) {
                handle.await.unwrap();
            }
        });
    });
}

fn spawn_many_remote_busy(b: &mut Bencher) {
    let rt = rt();
    let rt_handle = rt.handle();
    let mut handles = Vec::with_capacity(NUM_SPAWN);

    // Spawn some tasks to keep the runtimes busy
    for _ in 0..(2 * NUM_WORKERS) {
        rt.spawn(async {
            loop {
                tokio::task::yield_now().await;
                std::thread::sleep(std::time::Duration::from_micros(10));
            }
        });
    }

    b.iter(|| {
        for _ in 0..NUM_SPAWN {
            handles.push(rt_handle.spawn(async {}));
        }

        rt.block_on(async {
            for handle in handles.drain(..) {
                handle.await.unwrap();
            }
        });
    });
}

fn yield_many(b: &mut Bencher) {
    const NUM_YIELD: usize = 1_000;
    const TASKS: usize = 200;

    let rt = rt();

    let (tx, rx) = mpsc::sync_channel(TASKS);

    b.iter(move || {
        for _ in 0..TASKS {
            let tx = tx.clone();

            rt.spawn(async move {
                for _ in 0..NUM_YIELD {
                    tokio::task::yield_now().await;
                }

                tx.send(()).unwrap();
            });
        }

        for _ in 0..TASKS {
            let _ = rx.recv().unwrap();
        }
    });
}

fn ping_pong(b: &mut Bencher) {
    const NUM_PINGS: usize = 1_000;

    let rt = rt();

    let (done_tx, done_rx) = mpsc::sync_channel(1000);
    let rem = Arc::new(AtomicUsize::new(0));

    b.iter(|| {
        let done_tx = done_tx.clone();
        let rem = rem.clone();
        rem.store(NUM_PINGS, Relaxed);

        rt.block_on(async {
            tokio::spawn(async move {
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
    });
}

fn chained_spawn(b: &mut Bencher) {
    const ITER: usize = 1_000;

    let rt = rt();

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

        rt.block_on(async {
            tokio::spawn(async move {
                iter(done_tx, ITER);
            });

            done_rx.recv().unwrap();
        });
    });
}

fn rt() -> Runtime {
    runtime::Builder::new_multi_thread()
        .worker_threads(NUM_WORKERS)
        .enable_all()
        .build()
        .unwrap()
}

benchmark_group!(
    scheduler,
    spawn_many_local,
    spawn_many_remote_idle,
    spawn_many_remote_busy,
    ping_pong,
    yield_many,
    chained_spawn,
);

benchmark_main!(scheduler);
