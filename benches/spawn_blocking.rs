//! Benchmark spawn_blocking at different concurrency levels on the multi-threaded scheduler.
//!
//! For each parallelism level N (1, 2, 4, 8, 16, 32, 64, capped at available parallelism):
//! - Spawns N regular async tasks
//! - Each task spawns M batches of B spawn_blocking tasks (no-ops)
//! - Each batch is awaited to completion before starting the next
//!
//! Without `tokio_unstable`, set `TOKIO_UNSTABLE_SHARDED_BLOCKING_QUEUE=1`
//! to benchmark the sharded queue. Leave it unset to benchmark the locked queue.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::runtime::{self, Runtime};
use tokio::task::JoinSet;

/// Number of batches per task
const NUM_BATCHES: usize = 100;
/// Number of spawn_blocking calls per batch
const BATCH_SIZE: usize = 16;

fn spawn_blocking_concurrency(c: &mut Criterion) {
    let max_parallelism = std::thread::available_parallelism().map_or(1, |p| p.get());

    let parallelism_levels: Vec<usize> = [1, 2, 4, 8, 16, 32, 64]
        .into_iter()
        .filter(|&n| n <= max_parallelism)
        .collect();

    let mut group = c.benchmark_group("spawn_blocking");

    for num_tasks in parallelism_levels {
        group.bench_with_input(
            BenchmarkId::new("concurrency", num_tasks),
            &num_tasks,
            |b, &num_tasks| {
                let rt = rt();

                b.iter(|| {
                    rt.block_on(async {
                        let mut tasks = JoinSet::new();

                        for _ in 0..num_tasks {
                            tasks.spawn(async {
                                for _ in 0..NUM_BATCHES {
                                    let mut batch = JoinSet::new();

                                    for _ in 0..BATCH_SIZE {
                                        batch.spawn_blocking(|| black_box(0));
                                    }

                                    batch.join_all().await;
                                }
                            });
                        }

                        tasks.join_all().await;
                    });
                });
            },
        );
    }

    group.finish();
}

fn rt() -> Runtime {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn blocking_batch(producers: usize, short_work: bool) {
    let mut tasks = JoinSet::new();
    for _ in 0..producers {
        tasks.spawn(async move {
            let mut batch = JoinSet::new();
            for _ in 0..BATCH_SIZE {
                batch.spawn_blocking(move || {
                    if short_work {
                        for _ in 0..64 {
                            std::hint::spin_loop();
                        }
                    }
                    black_box(0)
                });
            }
            batch.join_all().await;
        });
    }
    tasks.join_all().await;
}

fn queue_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("blocking_queue_latency");
    #[cfg(tokio_unstable)]
    let queues = [false, true];
    #[cfg(not(tokio_unstable))]
    let queues = [std::env::var_os("TOKIO_UNSTABLE_SHARDED_BLOCKING_QUEUE")
        .is_some_and(|value| !value.is_empty() && value != "0")];
    for sharded in queues {
        for enabled in [false, true] {
            if enabled && !cfg!(all(tokio_unstable, target_has_atomic = "64")) {
                continue;
            }
            let mut builder = runtime::Builder::new_multi_thread();
            builder.worker_threads(4).max_blocking_threads(4);
            #[cfg(tokio_unstable)]
            if sharded {
                builder.enable_sharded_blocking_queue();
            }
            #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
            if enabled {
                builder.track_blocking_queue_latency();
            }
            let rt = builder.build().unwrap();
            for producers in [1, 4] {
                group.throughput(Throughput::Elements((producers * BATCH_SIZE) as u64));
                for short_work in [false, true] {
                    let name = format!(
                        "{}-{}-{}",
                        if sharded { "sharded" } else { "locked" },
                        if enabled { "enabled" } else { "disabled" },
                        if short_work { "short" } else { "noop" }
                    );
                    group.bench_with_input(
                        BenchmarkId::new(name, producers),
                        &producers,
                        |b, &producers| {
                            b.iter(|| rt.block_on(blocking_batch(producers, short_work)));
                        },
                    );
                }
            }
        }
    }
    group.finish();
}

criterion_group!(
    spawn_blocking_benches,
    spawn_blocking_concurrency,
    queue_latency
);

criterion_main!(spawn_blocking_benches);
