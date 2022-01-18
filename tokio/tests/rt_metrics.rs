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
    })
    .join()
    .unwrap();

    rt.block_on(task).unwrap();

    assert_eq!(1, rt.metrics().remote_schedule_count());

    let rt = threaded();
    let handle = rt.handle().clone();
    let task = thread::spawn(move || {
        handle.spawn(async {
            // DO nothing
        })
    })
    .join()
    .unwrap();

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
    assert!(1 <= metrics.worker_park_count(1));
}

#[test]
fn worker_noop_count() {
    // There isn't really a great way to generate no-op parks as they happen as
    // false-positive events under concurrency.

    let rt = basic();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert_eq!(2, metrics.worker_noop_count(0));

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert!(1 <= metrics.worker_noop_count(0));
    assert!(1 <= metrics.worker_noop_count(1));
}

#[test]
fn worker_steal_count() {
    // This metric only applies to the multi-threaded runtime.
    //
    // We use a blocking channel to backup one worker thread.
    use std::sync::mpsc::channel;

    let rt = threaded();
    let metrics = rt.metrics();

    rt.block_on(async {
        let (tx, rx) = channel();

        // Move to the runtime.
        tokio::spawn(async move {
            // Spawn the task that sends to the channel
            tokio::spawn(async move {
                tx.send(()).unwrap();
            });

            // Spawn a task that bumps the previous task out of the "next
            // scheduled" slot.
            tokio::spawn(async {});

            // Blocking receive on the channe.
            rx.recv().unwrap();
        })
        .await
        .unwrap();
    });

    drop(rt);

    let n: u64 = (0..metrics.num_workers())
        .map(|i| metrics.worker_steal_count(i))
        .sum();

    assert_eq!(1, n);
}

#[test]
fn worker_poll_count() {
    const N: u64 = 5;

    let rt = basic();
    let metrics = rt.metrics();
    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(async {}).await.unwrap();
        }
    });
    drop(rt);
    assert_eq!(N, metrics.worker_poll_count(0));

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(async {}).await.unwrap();
        }
    });
    drop(rt);
    // Account for the `block_on` task
    let n = (0..metrics.num_workers())
        .map(|i| metrics.worker_poll_count(i))
        .sum();

    assert_eq!(N, n);
}

#[test]
fn worker_total_busy_duration() {
    const N: usize = 5;

    let zero = Duration::from_millis(0);

    let rt = basic();
    let metrics = rt.metrics();

    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(async {
                tokio::task::yield_now().await;
            })
            .await
            .unwrap();
        }
    });

    drop(rt);

    assert!(zero < metrics.worker_total_busy_duration(0));

    let rt = threaded();
    let metrics = rt.metrics();

    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(async {
                tokio::task::yield_now().await;
            })
            .await
            .unwrap();
        }
    });

    drop(rt);

    for i in 0..metrics.num_workers() {
        assert!(zero < metrics.worker_total_busy_duration(i));
    }
}

#[test]
fn worker_local_schedule_count() {
    let rt = basic();
    let metrics = rt.metrics();
    rt.block_on(async {
        tokio::spawn(async {}).await.unwrap();
    });
    drop(rt);

    assert_eq!(1, metrics.worker_local_schedule_count(0));
    assert_eq!(0, metrics.remote_schedule_count());
}

#[test]
#[ignore]
fn worker_overflow_count() {}

#[test]
#[ignore]
fn remote_queue_depth() {}

#[test]
#[ignore]
fn worker_local_queue_depth() {}

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
