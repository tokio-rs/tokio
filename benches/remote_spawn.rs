//! Benchmark remote task spawning (push_remote_task) at different concurrency
//! levels on the multi-threaded scheduler.
//!
//! This measures contention on the scheduler's inject queue mutex when multiple
//! external (non-worker) threads spawn tasks into the tokio runtime simultaneously.
//! Every rt.spawn() from an external thread unconditionally goes through
//! push_remote_task, making this a direct measurement of inject queue contention.
//!
//! For each parallelism level N (1, 2, 4, 8, 16, 32, 64, capped at available parallelism):
//! - Spawns N std::threads (external to the runtime)
//! - Each thread spawns TOTAL_TASKS / N tasks into the runtime via rt.spawn()
//! - All threads are synchronized with a barrier to maximize contention
//! - Tasks are trivial no-ops to isolate the push overhead

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::sync::Barrier;
use tokio::runtime::{self, Runtime};

/// Total number of tasks spawned across all threads per iteration.
/// Must be divisible by the largest parallelism level (64).
const TOTAL_TASKS: usize = 12_800;

fn remote_spawn_contention(c: &mut Criterion) {
    let parallelism_levels = parallelism_levels();
    let mut group = c.benchmark_group("remote_spawn");

    for num_threads in &parallelism_levels {
        let num_threads = *num_threads;
        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, &num_threads| {
                let rt = rt();
                let tasks_per_thread = TOTAL_TASKS / num_threads;

                b.iter(|| {
                    let barrier = Barrier::new(num_threads);

                    std::thread::scope(|s| {
                        let handles: Vec<_> = (0..num_threads)
                            .map(|_| {
                                let barrier = &barrier;
                                let rt = &rt;
                                s.spawn(move || {
                                    let mut join_handles = Vec::with_capacity(tasks_per_thread);
                                    barrier.wait();

                                    for _ in 0..tasks_per_thread {
                                        join_handles.push(rt.spawn(async {}));
                                    }
                                    join_handles
                                })
                            })
                            .collect();

                        let all_handles: Vec<_> = handles
                            .into_iter()
                            .flat_map(|h| h.join().unwrap())
                            .collect();

                        rt.block_on(async {
                            for h in all_handles {
                                h.await.unwrap();
                            }
                        });
                    });
                });
            },
        );
    }

    group.finish();
}

fn parallelism_levels() -> Vec<usize> {
    let max_parallelism = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1);

    [1, 2, 4, 8, 16, 32, 64]
        .into_iter()
        .filter(|&n| n <= max_parallelism)
        .collect()
}

fn rt() -> Runtime {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

criterion_group!(remote_spawn_benches, remote_spawn_contention);

criterion_main!(remote_spawn_benches);
