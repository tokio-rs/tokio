/// Benchmark measuring timer lifecycle performance (Issue #6504)
///
/// This benchmark creates many timers, polls them once to register with the timer
/// system, then drops them before they fire. This simulates the common case of
/// timeouts that don't fire (e.g., operations completing before timeout).
///
/// The benchmark compares single-threaded vs multi-threaded performance to reveal
/// contention in timer registration and deregistration.
use std::future::{poll_fn, Future};
use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::{runtime::Runtime, time::sleep};

const TIMER_COUNT: usize = 10_000;

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

/// Returns (wall_clock_duration, per_task_durations)
async fn create_and_drop_timers_instrumented(count: usize, workers: usize) -> (Duration, Vec<Duration>) {
    let handles: Vec<_> = (0..workers)
        .map(|_| {
            tokio::spawn(async move {
                // Create all sleep futures
                let mut sleeps = Vec::with_capacity(count / workers);
                for _ in 0..count / workers {
                    sleeps.push(Box::pin(sleep(Duration::from_secs(60))));
                }

                // Start timing - poll and drop (METERED)
                let start = Instant::now();
                for mut sleep in sleeps {
                    // Poll once to register
                    poll_fn(|cx| {
                        let _ = sleep.as_mut().poll(cx);
                        std::task::Poll::Ready(())
                    })
                    .await;

                    // Drop to deregister
                    black_box(drop(sleep));
                }
                let elapsed = start.elapsed();

                elapsed
            })
        })
        .collect();

    let wall_clock_start = Instant::now();

    let mut task_durations = Vec::with_capacity(workers);
    for handle in handles {
        task_durations.push(handle.await.unwrap());
    }

    let wall_clock = wall_clock_start.elapsed();

    (wall_clock, task_durations)
}

fn bench_many_timers(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_timers");

    // Single-threaded baseline
    let runtime = build_runtime(1);
    group.bench_function("single_thread", |b| {
        b.iter_custom(|iters| {
            let (wall_clock, _task_durations) = runtime.block_on(async {
                create_and_drop_timers_instrumented(TIMER_COUNT * iters as usize, 1).await
            });

            wall_clock
        })
    });

    // Multi-threaded with 8 workers
    let runtime_multi = build_runtime(8);
    group.bench_function("multi_thread", |b| {
        b.iter_custom(|iters| {
            let (wall_clock, task_durations) = runtime_multi.block_on(async {
                create_and_drop_timers_instrumented(TIMER_COUNT * iters as usize, 8).await
            });

            // Print variance stats to stderr
            let min = task_durations.iter().min().unwrap();
            let max = task_durations.iter().max().unwrap();
            let range = max.saturating_sub(*min);
            eprintln!(
                "multi_thread: wall={:?}, min={:?}, max={:?}, range={:?}",
                wall_clock, min, max, range
            );

            wall_clock
        })
    });

    group.finish();
}

criterion_group!(
    many_timers,
    bench_many_timers
);

criterion_main!(many_timers);
