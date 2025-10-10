/// Benchmark demonstrating timer mutex contention on drop (Issue #6504)
///
/// This benchmark creates many timers, polls them once to initialize and register
/// them with the timer wheel, then drops them before they fire. This is the common
/// case for timeouts that are set but don't fire.
///
/// Each drop acquires the global timer mutex to deregister from the wheel, causing
/// severe contention under concurrent load.
///
/// ## Baseline Results (Pre-Fix)
///
/// ```text
/// timer_drop_single_thread_10k:         33.3 ms (32.7-34.0 ms)
/// timer_drop_multi_thread_10k_8workers: 21.6 ms (19.1-24.7 ms)
/// ```
///
/// **Analysis**: Multi-threaded (8 workers) is only 1.54x faster than single-threaded,
/// demonstrating severe mutex contention.

use std::future::{poll_fn, Future};
use std::pin::Pin;
use std::time::Instant;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::{
    runtime::Runtime,
    time::{sleep, Duration},
};

fn build_runtime(workers: usize) -> Runtime {
    if workers == 1 {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(workers)
            .build()
            .unwrap()
    }
}

async fn create_and_drop_timers(count: usize, workers: usize) {
    let handles: Vec<_> = (0..workers)
        .map(|_| {
            tokio::spawn(async move {
                for _ in 0..count / workers {
                    let mut sleep = Box::pin(sleep(Duration::from_secs(60)));

                    // Poll once to initialize and register without awaiting
                    poll_fn(|cx| {
                        let _ = sleep.as_mut().poll(cx);
                        std::task::Poll::Ready(())
                    })
                    .await;

                    black_box(drop(sleep));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }
}

fn timer_drop_contention_single_thread(c: &mut Criterion) {
    let runtime = build_runtime(1);

    c.bench_function("timer_drop_single_thread_10k", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(create_and_drop_timers(10_000 * iters as usize, 1)).await;
            });
            start.elapsed()
        })
    });
}

fn timer_drop_contention_multi_thread(c: &mut Criterion) {
    let runtime = build_runtime(8);

    c.bench_function("timer_drop_multi_thread_10k_8workers", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(create_and_drop_timers(10_000 * iters as usize, 8)).await;
            });
            start.elapsed()
        })
    });
}

criterion_group!(
    timer_contention,
    timer_drop_contention_single_thread,
    timer_drop_contention_multi_thread
);

criterion_main!(timer_contention);
