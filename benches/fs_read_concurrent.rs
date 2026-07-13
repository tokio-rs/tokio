//! Benchmark concurrent `tokio::fs::read` calls on the multi-threaded scheduler.
//!
//! For each concurrency level N (1, 2, 4, 8, 16, 32, 64, capped at available
//! parallelism):
//! - Spawns N async tasks
//! - Each task reads its own small file NUM_READS times
//!
//! By default, file operations run on the `spawn_blocking` pool. To route them
//! through io_uring instead, build with the `io-uring` feature and the
//! `tokio_unstable` cfg:
//!
//! ```sh
//! RUSTFLAGS="--cfg tokio_unstable" cargo bench --bench fs_read_concurrent --features io-uring
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::runtime::{self, Runtime};
use tokio::task::JoinSet;

use std::path::PathBuf;

/// Number of reads each task performs per iteration
const NUM_READS: usize = 100;
/// Size of each file in bytes
const FILE_SIZE: usize = 4096;

fn fs_read_concurrency(c: &mut Criterion) {
    let max_parallelism = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1);

    let concurrency_levels: Vec<usize> = [1, 2, 4, 8, 16, 32, 64]
        .into_iter()
        .filter(|&n| n <= max_parallelism)
        .collect();

    let dir = std::env::temp_dir().join(format!("tokio-fs-read-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let max_tasks = concurrency_levels.iter().copied().max().unwrap_or(1);
    let paths: Vec<PathBuf> = (0..max_tasks)
        .map(|i| {
            let path = dir.join(format!("file-{i}"));
            std::fs::write(&path, vec![0u8; FILE_SIZE]).unwrap();
            path
        })
        .collect();

    let mut group = c.benchmark_group("fs_read");

    for num_tasks in concurrency_levels {
        group.bench_with_input(
            BenchmarkId::new("concurrency", num_tasks),
            &num_tasks,
            |b, &num_tasks| {
                let rt = rt();

                b.iter(|| {
                    rt.block_on(async {
                        let mut tasks = JoinSet::new();

                        for path in paths.iter().take(num_tasks).cloned() {
                            tasks.spawn(async move {
                                for _ in 0..NUM_READS {
                                    let data = tokio::fs::read(&path).await.unwrap();
                                    assert_eq!(data.len(), FILE_SIZE);
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

    std::fs::remove_dir_all(&dir).unwrap();
}

fn rt() -> Runtime {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

criterion_group!(fs_read_benches, fs_read_concurrency);

criterion_main!(fs_read_benches);
