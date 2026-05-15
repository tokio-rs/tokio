//! Benchmark spawn_blocking at different concurrency levels on the multi-threaded scheduler.
//!
//! For each parallelism level N (1, 2, 4, 8, 16, 32, 64, capped at available parallelism):
//! - Spawns N regular async tasks
//! - Each task spawns M batches of B spawn_blocking tasks (no-ops)
//! - Each batch is awaited to completion before starting the next

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::runtime::{self, Runtime};
use tokio::task::JoinSet;

/// Number of batches per task
const NUM_BATCHES: usize = 100;
/// Number of spawn_blocking calls per batch
const BATCH_SIZE: usize = 16;

fn spawn_blocking_concurrency(c: &mut Criterion) {
    let max_parallelism = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1);

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

criterion_group!(spawn_blocking_benches, spawn_blocking_concurrency);

criterion_main!(spawn_blocking_benches);
