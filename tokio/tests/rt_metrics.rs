#![allow(unknown_lints, unexpected_cfgs)]
#![warn(rust_2018_idioms)]
#![cfg(all(
    feature = "full",
    tokio_unstable,
    not(target_os = "wasi"),
    target_has_atomic = "64"
))]

use std::future::Future;
use std::sync::{Arc, Barrier, Mutex};
use std::task::Poll;
use tokio::macros::support::poll_fn;

use tokio::runtime::Runtime;
use tokio::task::consume_budget;
use tokio::time::{self, Duration};

#[test]
fn num_workers() {
    let rt = current_thread();
    assert_eq!(1, rt.metrics().num_workers());

    let rt = threaded();
    assert_eq!(2, rt.metrics().num_workers());
}

#[test]
fn num_blocking_threads() {
    let rt = current_thread();
    assert_eq!(0, rt.metrics().num_blocking_threads());
    let _ = rt.block_on(rt.spawn_blocking(move || {}));
    assert_eq!(1, rt.metrics().num_blocking_threads());

    let rt = threaded();
    assert_eq!(0, rt.metrics().num_blocking_threads());
    let _ = rt.block_on(rt.spawn_blocking(move || {}));
    assert_eq!(1, rt.metrics().num_blocking_threads());
}

#[test]
fn num_idle_blocking_threads() {
    let rt = current_thread();
    assert_eq!(0, rt.metrics().num_idle_blocking_threads());
    let _ = rt.block_on(rt.spawn_blocking(move || {}));
    rt.block_on(async {
        time::sleep(Duration::from_millis(5)).await;
    });

    // We need to wait until the blocking thread has become idle. Usually 5ms is
    // enough for this to happen, but not always. When it isn't enough, sleep
    // for another second. We don't always wait for a whole second since we want
    // the test suite to finish quickly.
    //
    // Note that the timeout for idle threads to be killed is 10 seconds.
    if 0 == rt.metrics().num_idle_blocking_threads() {
        rt.block_on(async {
            time::sleep(Duration::from_secs(1)).await;
        });
    }

    assert_eq!(1, rt.metrics().num_idle_blocking_threads());
}

#[test]
fn blocking_queue_depth() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();

    assert_eq!(0, rt.metrics().blocking_queue_depth());

    let ready = Arc::new(Mutex::new(()));
    let guard = ready.lock().unwrap();

    let ready_cloned = ready.clone();
    let wait_until_ready = move || {
        let _unused = ready_cloned.lock().unwrap();
    };

    let h1 = rt.spawn_blocking(wait_until_ready.clone());
    let h2 = rt.spawn_blocking(wait_until_ready);
    assert!(rt.metrics().blocking_queue_depth() > 0);

    drop(guard);

    let _ = rt.block_on(h1);
    let _ = rt.block_on(h2);

    assert_eq!(0, rt.metrics().blocking_queue_depth());
}

#[test]
fn active_tasks_count() {
    let rt = current_thread();
    let metrics = rt.metrics();
    assert_eq!(0, metrics.active_tasks_count());
    rt.spawn(async move {
        assert_eq!(1, metrics.active_tasks_count());
    });

    let rt = threaded();
    let metrics = rt.metrics();
    assert_eq!(0, metrics.active_tasks_count());
    rt.spawn(async move {
        assert_eq!(1, metrics.active_tasks_count());
    });
}

#[test]
fn remote_schedule_count() {
    use std::thread;

    let rt = current_thread();
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
    let rt = current_thread();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert!(1 <= metrics.worker_park_count(0));

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

    let rt = current_thread();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert!(0 < metrics.worker_noop_count(0));

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        time::sleep(Duration::from_millis(1)).await;
    });
    drop(rt);
    assert!(0 < metrics.worker_noop_count(0));
    assert!(0 < metrics.worker_noop_count(1));
}

#[test]
#[ignore] // this test is flaky, see https://github.com/tokio-rs/tokio/issues/6470
fn worker_steal_count() {
    // This metric only applies to the multi-threaded runtime.
    //
    // We use a blocking channel to backup one worker thread.
    use std::sync::mpsc::channel;

    let rt = threaded_no_lifo();
    let metrics = rt.metrics();

    rt.block_on(async {
        let (tx, rx) = channel();

        // Move to the runtime.
        tokio::spawn(async move {
            // Spawn the task that sends to the channel
            //
            // Since the lifo slot is disabled, this task is stealable.
            tokio::spawn(async move {
                tx.send(()).unwrap();
            });

            // Blocking receive on the channel.
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
fn worker_poll_count_and_time() {
    const N: u64 = 5;

    async fn task() {
        // Sync sleep
        std::thread::sleep(std::time::Duration::from_micros(10));
    }

    let rt = current_thread();
    let metrics = rt.metrics();
    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(task()).await.unwrap();
        }
    });
    drop(rt);
    assert_eq!(N, metrics.worker_poll_count(0));
    // Not currently supported for current-thread runtime
    assert_eq!(Duration::default(), metrics.worker_mean_poll_time(0));

    // Does not populate the histogram
    assert!(!metrics.poll_count_histogram_enabled());
    for i in 0..10 {
        assert_eq!(0, metrics.poll_count_histogram_bucket_count(0, i));
    }

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(task()).await.unwrap();
        }
    });
    drop(rt);
    // Account for the `block_on` task
    let n = (0..metrics.num_workers())
        .map(|i| metrics.worker_poll_count(i))
        .sum();

    assert_eq!(N, n);

    let n: Duration = (0..metrics.num_workers())
        .map(|i| metrics.worker_mean_poll_time(i))
        .sum();

    assert!(n > Duration::default());

    // Does not populate the histogram
    assert!(!metrics.poll_count_histogram_enabled());
    for n in 0..metrics.num_workers() {
        for i in 0..10 {
            assert_eq!(0, metrics.poll_count_histogram_bucket_count(n, i));
        }
    }
}

#[test]
fn worker_poll_count_histogram() {
    const N: u64 = 5;

    let rts = [
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .enable_metrics_poll_count_histogram()
            .metrics_poll_count_histogram_scale(tokio::runtime::HistogramScale::Linear)
            .metrics_poll_count_histogram_buckets(3)
            .metrics_poll_count_histogram_resolution(Duration::from_millis(50))
            .build()
            .unwrap(),
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .enable_metrics_poll_count_histogram()
            .metrics_poll_count_histogram_scale(tokio::runtime::HistogramScale::Linear)
            .metrics_poll_count_histogram_buckets(3)
            .metrics_poll_count_histogram_resolution(Duration::from_millis(50))
            .build()
            .unwrap(),
    ];

    for rt in rts {
        let metrics = rt.metrics();
        rt.block_on(async {
            for _ in 0..N {
                tokio::spawn(async {}).await.unwrap();
            }
        });
        drop(rt);

        let num_workers = metrics.num_workers();
        let num_buckets = metrics.poll_count_histogram_num_buckets();

        assert!(metrics.poll_count_histogram_enabled());
        assert_eq!(num_buckets, 3);

        let n = (0..num_workers)
            .flat_map(|i| (0..num_buckets).map(move |j| (i, j)))
            .map(|(worker, bucket)| metrics.poll_count_histogram_bucket_count(worker, bucket))
            .sum();
        assert_eq!(N, n);
    }
}

#[test]
fn worker_poll_count_histogram_range() {
    let max = Duration::from_nanos(u64::MAX);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .enable_metrics_poll_count_histogram()
        .metrics_poll_count_histogram_scale(tokio::runtime::HistogramScale::Linear)
        .metrics_poll_count_histogram_buckets(3)
        .metrics_poll_count_histogram_resolution(us(50))
        .build()
        .unwrap();
    let metrics = rt.metrics();

    assert_eq!(metrics.poll_count_histogram_bucket_range(0), us(0)..us(50));
    assert_eq!(
        metrics.poll_count_histogram_bucket_range(1),
        us(50)..us(100)
    );
    assert_eq!(metrics.poll_count_histogram_bucket_range(2), us(100)..max);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .enable_metrics_poll_count_histogram()
        .metrics_poll_count_histogram_scale(tokio::runtime::HistogramScale::Log)
        .metrics_poll_count_histogram_buckets(3)
        .metrics_poll_count_histogram_resolution(us(50))
        .build()
        .unwrap();
    let metrics = rt.metrics();

    let a = Duration::from_nanos(50000_u64.next_power_of_two());
    let b = a * 2;

    assert_eq!(metrics.poll_count_histogram_bucket_range(0), us(0)..a);
    assert_eq!(metrics.poll_count_histogram_bucket_range(1), a..b);
    assert_eq!(metrics.poll_count_histogram_bucket_range(2), b..max);
}

#[test]
fn worker_poll_count_histogram_disabled_without_explicit_enable() {
    let rts = [
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .metrics_poll_count_histogram_scale(tokio::runtime::HistogramScale::Linear)
            .metrics_poll_count_histogram_buckets(3)
            .metrics_poll_count_histogram_resolution(Duration::from_millis(50))
            .build()
            .unwrap(),
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .metrics_poll_count_histogram_scale(tokio::runtime::HistogramScale::Linear)
            .metrics_poll_count_histogram_buckets(3)
            .metrics_poll_count_histogram_resolution(Duration::from_millis(50))
            .build()
            .unwrap(),
    ];

    for rt in rts {
        let metrics = rt.metrics();
        assert!(!metrics.poll_count_histogram_enabled());
    }
}

#[test]
fn worker_total_busy_duration() {
    const N: usize = 5;

    let zero = Duration::from_millis(0);

    let rt = current_thread();
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
    let rt = current_thread();
    let metrics = rt.metrics();
    rt.block_on(async {
        tokio::spawn(async {}).await.unwrap();
    });
    drop(rt);

    assert_eq!(1, metrics.worker_local_schedule_count(0));
    assert_eq!(0, metrics.remote_schedule_count());

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        // Move to the runtime
        tokio::spawn(async {
            tokio::spawn(async {}).await.unwrap();
        })
        .await
        .unwrap();
    });
    drop(rt);

    let n: u64 = (0..metrics.num_workers())
        .map(|i| metrics.worker_local_schedule_count(i))
        .sum();

    assert_eq!(2, n);
    assert_eq!(1, metrics.remote_schedule_count());
}

#[test]
fn worker_overflow_count() {
    // Only applies to the threaded worker
    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async {
        // Move to the runtime
        tokio::spawn(async {
            let (tx1, rx1) = std::sync::mpsc::channel();
            let (tx2, rx2) = std::sync::mpsc::channel();

            // First, we need to block the other worker until all tasks have
            // been spawned.
            //
            // We spawn from outside the runtime to ensure that the other worker
            // will pick it up:
            // <https://github.com/tokio-rs/tokio/issues/4730>
            tokio::task::spawn_blocking(|| {
                tokio::spawn(async move {
                    tx1.send(()).unwrap();
                    rx2.recv().unwrap();
                });
            });

            rx1.recv().unwrap();

            // Spawn many tasks
            for _ in 0..300 {
                tokio::spawn(async {});
            }

            tx2.send(()).unwrap();
        })
        .await
        .unwrap();
    });
    drop(rt);

    let n: u64 = (0..metrics.num_workers())
        .map(|i| metrics.worker_overflow_count(i))
        .sum();

    assert_eq!(1, n);
}

#[test]
fn injection_queue_depth_current_thread() {
    use std::thread;

    let rt = current_thread();
    let handle = rt.handle().clone();
    let metrics = rt.metrics();

    thread::spawn(move || {
        handle.spawn(async {});
    })
    .join()
    .unwrap();

    assert_eq!(1, metrics.injection_queue_depth());
}

#[test]
fn injection_queue_depth_multi_thread() {
    let rt = threaded();
    let metrics = rt.metrics();

    let barrier1 = Arc::new(Barrier::new(3));
    let barrier2 = Arc::new(Barrier::new(3));

    // Spawn a task per runtime worker to block it.
    for _ in 0..2 {
        let barrier1 = barrier1.clone();
        let barrier2 = barrier2.clone();
        rt.spawn(async move {
            barrier1.wait();
            barrier2.wait();
        });
    }

    barrier1.wait();

    for i in 0..10 {
        assert_eq!(i, metrics.injection_queue_depth());
        rt.spawn(async {});
    }

    barrier2.wait();
}

#[test]
fn worker_local_queue_depth() {
    const N: usize = 100;

    let rt = current_thread();
    let metrics = rt.metrics();
    rt.block_on(async {
        for _ in 0..N {
            tokio::spawn(async {});
        }

        assert_eq!(N, metrics.worker_local_queue_depth(0));
    });

    let rt = threaded();
    let metrics = rt.metrics();
    rt.block_on(async move {
        // Move to the runtime
        tokio::spawn(async move {
            let (tx1, rx1) = std::sync::mpsc::channel();
            let (tx2, rx2) = std::sync::mpsc::channel();

            // First, we need to block the other worker until all tasks have
            // been spawned.
            tokio::spawn(async move {
                tx1.send(()).unwrap();
                rx2.recv().unwrap();
            });

            // Bump the next-run spawn
            tokio::spawn(async {});

            rx1.recv().unwrap();

            // Spawn some tasks
            for _ in 0..100 {
                tokio::spawn(async {});
            }

            let n: usize = (0..metrics.num_workers())
                .map(|i| metrics.worker_local_queue_depth(i))
                .sum();

            assert_eq!(n, N);

            tx2.send(()).unwrap();
        })
        .await
        .unwrap();
    });
}

#[test]
fn budget_exhaustion_yield() {
    let rt = current_thread();
    let metrics = rt.metrics();

    assert_eq!(0, metrics.budget_forced_yield_count());

    let mut did_yield = false;

    // block on a task which consumes budget until it yields
    rt.block_on(poll_fn(|cx| loop {
        if did_yield {
            return Poll::Ready(());
        }

        let fut = consume_budget();
        tokio::pin!(fut);

        if fut.poll(cx).is_pending() {
            did_yield = true;
            return Poll::Pending;
        }
    }));

    assert_eq!(1, rt.metrics().budget_forced_yield_count());
}

#[test]
fn budget_exhaustion_yield_with_joins() {
    let rt = current_thread();
    let metrics = rt.metrics();

    assert_eq!(0, metrics.budget_forced_yield_count());

    let mut did_yield_1 = false;
    let mut did_yield_2 = false;

    // block on a task which consumes budget until it yields
    rt.block_on(async {
        tokio::join!(
            poll_fn(|cx| loop {
                if did_yield_1 {
                    return Poll::Ready(());
                }

                let fut = consume_budget();
                tokio::pin!(fut);

                if fut.poll(cx).is_pending() {
                    did_yield_1 = true;
                    return Poll::Pending;
                }
            }),
            poll_fn(|cx| loop {
                if did_yield_2 {
                    return Poll::Ready(());
                }

                let fut = consume_budget();
                tokio::pin!(fut);

                if fut.poll(cx).is_pending() {
                    did_yield_2 = true;
                    return Poll::Pending;
                }
            })
        )
    });

    assert_eq!(1, rt.metrics().budget_forced_yield_count());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn io_driver_fd_count() {
    let rt = current_thread();
    let metrics = rt.metrics();

    assert_eq!(metrics.io_driver_fd_registered_count(), 0);

    let stream = tokio::net::TcpStream::connect("google.com:80");
    let stream = rt.block_on(async move { stream.await.unwrap() });

    assert_eq!(metrics.io_driver_fd_registered_count(), 1);
    assert_eq!(metrics.io_driver_fd_deregistered_count(), 0);

    drop(stream);

    assert_eq!(metrics.io_driver_fd_deregistered_count(), 1);
    assert_eq!(metrics.io_driver_fd_registered_count(), 1);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn io_driver_ready_count() {
    let rt = current_thread();
    let metrics = rt.metrics();

    let stream = tokio::net::TcpStream::connect("google.com:80");
    let _stream = rt.block_on(async move { stream.await.unwrap() });

    assert_eq!(metrics.io_driver_ready_count(), 1);
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

fn threaded_no_lifo() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .disable_lifo_slot()
        .enable_all()
        .build()
        .unwrap()
}

fn us(n: u64) -> Duration {
    Duration::from_micros(n)
}
