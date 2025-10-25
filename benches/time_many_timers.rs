use criterion::{black_box, criterion_group, criterion_main, Criterion};
/// Benchmark measuring timer lifecycle performance (Issue #6504)
///
/// This benchmark creates many timers, polls them once to register with the timer
/// system, then drops them before they fire. This simulates the common case of
/// timeouts that don't fire (e.g., operations completing before timeout).
///
/// The benchmark compares single-threaded vs multi-threaded performance to reveal
/// contention in timer registration and deregistration.
use std::future::{poll_fn, Future};
use std::iter::repeat;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const TIMER_COUNT: usize = 1_000_000;

struct TimerDistribution {
    duration: Duration,
    percentage: f64,
}

const fn from_secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

const TIMER_DISTRIBUTIONS: &[TimerDistribution] = &[
    TimerDistribution {
        duration: from_secs(1),
        percentage: 0.40,
    },
    TimerDistribution {
        duration: from_secs(10),
        percentage: 0.30,
    },
    TimerDistribution {
        duration: from_secs(60),
        percentage: 0.20,
    },
    TimerDistribution {
        duration: from_secs(300),
        percentage: 0.10,
    },
];

/// Each timer is polled once to register, then dropped before firing.
async fn create_and_drop_timers_instrumented(count: usize, concurrent_tasks: usize) -> Duration {
    let handles: Vec<_> = (0..concurrent_tasks)
        .map(|_| {
            tokio::spawn(async move {
                // Create all sleep futures with realistic distribution
                let sleeps: Vec<_> = TIMER_DISTRIBUTIONS
                    .iter()
                    .flat_map(|td| {
                        repeat(td.duration).take((TIMER_COUNT as f64 * td.percentage) as usize)
                    })
                    .cycle()
                    .take(count / concurrent_tasks)
                    .map(|timeout| Box::pin(sleep(timeout)))
                    .collect();

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

    for handle in handles {
        handle.await.unwrap();
    }

    wall_clock_start.elapsed()
}

fn bench_many_timers(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_timers");

    // Single-threaded baseline
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    group.bench_function("single_thread", |b| {
        b.iter_custom(|_iters| {
            let wall_clock = runtime
                .block_on(async { create_and_drop_timers_instrumented(TIMER_COUNT, 1).await });

            wall_clock
        })
    });

    // Multi-threaded with 8 workers
    let runtime_multi = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(8)
        .build()
        .unwrap();

    group.bench_function("multi_thread", |b| {
        b.iter_custom(|_iters| {
            let wall_clock = runtime_multi
                .block_on(async { create_and_drop_timers_instrumented(TIMER_COUNT, 8).await });

            wall_clock
        })
    });

    group.finish();
}

criterion_group!(many_timers, bench_many_timers);

criterion_main!(many_timers);
