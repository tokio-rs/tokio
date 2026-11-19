#![cfg(unix)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use std::io::{Read, Write};
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, FramedRead};

fn rt_current_thread() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn rt_multi_thread() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

/// Multi-thread runtime with a small blocking thread pool.
fn rt_limited_blocking() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .max_blocking_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

const FILE_SIZES: &[(usize, &str)] = &[
    (64 * 1024, "64KiB"),
    (1024 * 1024, "1MiB"),
    (16 * 1024 * 1024, "16MiB"),
    (256 * 1024 * 1024, "256MiB"),
];

const STREAM_BUF_SIZE: usize = 64 * 1024;

fn create_temp_file(size: usize) -> (NamedTempFile, std::path::PathBuf) {
    // Use the current directory (typically on disk) rather than
    // `/tmp` (often RAM-backed tmpfs), so benchmarks measure actual I/O
    let mut tmp = NamedTempFile::new_in(".").unwrap();
    let chunk: Vec<u8> = (0u8..=255).collect();
    let mut remaining = size;
    while remaining > 0 {
        let n = remaining.min(chunk.len());
        tmp.write_all(&chunk[..n]).unwrap();
        remaining -= n;
    }
    tmp.flush().unwrap();
    let path = tmp.path().to_path_buf();
    (tmp, path)
}

/// Open a file and read it to completion in `buf_size` chunks via AsyncRead.
async fn stream_read(path: std::path::PathBuf, buf_size: usize) {
    let mut file = File::open(&path).await.unwrap();
    let mut buffer = vec![0u8; buf_size];
    loop {
        let n = file.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        black_box(&buffer[..n]);
    }
}

/*
 * /dev/zero micro-benchmarks: measure pure async overhead without real I/O.
 */

const BLOCK_COUNT: usize = 1_000;
const BUFFER_SIZE: usize = 4096;
const DEV_ZERO: &str = "/dev/zero";

fn bench_devzero(c: &mut Criterion) {
    let mut group = c.benchmark_group("devzero");

    group.bench_function("async_read_codec", |b| {
        let rt = rt_multi_thread();
        b.iter(|| {
            rt.block_on(async {
                let file = File::open(DEV_ZERO).await.unwrap();
                let mut input_stream =
                    FramedRead::with_capacity(file, BytesCodec::new(), BUFFER_SIZE);
                for _i in 0..BLOCK_COUNT {
                    let _bytes = input_stream.next().await.unwrap();
                }
            })
        });
    });

    group.bench_function("async_read_buf", |b| {
        let rt = rt_multi_thread();
        b.iter(|| {
            rt.block_on(async {
                let mut file = File::open(DEV_ZERO).await.unwrap();
                let mut buffer = [0u8; BUFFER_SIZE];
                for _i in 0..BLOCK_COUNT {
                    let count = file.read(&mut buffer).await.unwrap();
                    if count == 0 {
                        break;
                    }
                }
            })
        });
    });

    group.bench_function("async_read_std_file", |b| {
        let rt = rt_multi_thread();
        b.iter(|| {
            rt.block_on(async {
                let mut file = tokio::task::block_in_place(|| {
                    Box::pin(std::fs::File::open(DEV_ZERO).unwrap())
                });
                for _i in 0..BLOCK_COUNT {
                    let mut buffer = [0u8; BUFFER_SIZE];
                    let mut file_ref = file.as_mut();
                    tokio::task::block_in_place(move || {
                        file_ref.read_exact(&mut buffer).unwrap();
                    });
                }
            })
        });
    });

    group.bench_function("sync_read", |b| {
        b.iter(|| {
            let mut file = std::fs::File::open(DEV_ZERO).unwrap();
            let mut buffer = [0u8; BUFFER_SIZE];
            for _i in 0..BLOCK_COUNT {
                file.read_exact(&mut buffer).unwrap();
            }
        });
    });

    group.finish();
}

/*
 * Real-file benchmarks: varied sizes, runtimes, and concurrency levels.
 */

/// Benchmark `tokio::fs::read()` and `File` streaming across file sizes.
fn bench_sequential_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_read");

    for &(size, label) in FILE_SIZES {
        let (_tmp, path) = create_temp_file(size);

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("fs_read/multi_thread", label),
            &path,
            |b, path| {
                let rt = rt_multi_thread();
                b.iter(|| {
                    rt.block_on(async {
                        let data = tokio::fs::read(path).await.unwrap();
                        black_box(data);
                    })
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("fs_read/current_thread", label),
            &path,
            |b, path| {
                let rt = rt_current_thread();
                b.iter(|| {
                    rt.block_on(async {
                        let data = tokio::fs::read(path).await.unwrap();
                        black_box(data);
                    })
                });
            },
        );

        let buf_size = STREAM_BUF_SIZE;
        group.bench_with_input(
            BenchmarkId::new("file_stream/multi_thread", label),
            &path,
            |b, path| {
                let rt = rt_multi_thread();
                b.iter(|| rt.block_on(stream_read(path.to_path_buf(), buf_size)));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("file_stream/current_thread", label),
            &path,
            |b, path| {
                let rt = rt_current_thread();
                b.iter(|| rt.block_on(stream_read(path.to_path_buf(), buf_size)));
            },
        );

        // Sync streaming baseline: same buffer size, same read loop,
        // no async overhead. The floor for what file_stream can achieve.
        group.bench_with_input(BenchmarkId::new("sync_stream", label), &path, |b, path| {
            b.iter(|| {
                let mut file = std::fs::File::open(path).unwrap();
                let mut buffer = vec![0u8; buf_size];
                loop {
                    let n = file.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    black_box(&buffer[..n]);
                }
            })
        });
    }

    group.finish();
}

/// Effect of buffer size on `File` streaming throughput (1 MiB file).
fn bench_buffer_size(c: &mut Criterion) {
    const FILE_SIZE: usize = 1024 * 1024; // 1 MiB
    let (_tmp, path) = create_temp_file(FILE_SIZE);

    let buf_sizes: &[(usize, &str)] = &[
        (512, "512B"),
        (4096, "4KiB"),
        (32 * 1024, "32KiB"),
        (128 * 1024, "128KiB"),
        (1024 * 1024, "1MiB"),
    ];

    let mut group = c.benchmark_group("buffer_size");
    group.throughput(Throughput::Bytes(FILE_SIZE as u64));

    for &(buf_size, label) in buf_sizes {
        group.bench_with_input(
            BenchmarkId::new("async_read", label),
            &buf_size,
            |b, &buf_size| {
                let rt = rt_multi_thread();
                let path = path.clone();
                b.iter(|| rt.block_on(stream_read(path.clone(), buf_size)));
            },
        );
    }

    group.finish();
}

/// Sync baseline using `std::fs::read()` for comparison.
fn bench_sync_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync_read");

    for &(size, label) in FILE_SIZES {
        let (_tmp, path) = create_temp_file(size);

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("std_fs_read", label), &path, |b, path| {
            b.iter(|| {
                let data = std::fs::read(path).unwrap();
                black_box(data);
            })
        });
    }

    group.finish();
}

/// Concurrent `fs::read()` from multiple files using `JoinSet`.
fn bench_concurrent_reads(c: &mut Criterion) {
    let concurrency_levels: &[usize] = &[4, 16, 64];
    let file_size: usize = 256 * 1024 * 1024; // 256 MiB

    let mut group = c.benchmark_group("concurrent_reads");

    for &n in concurrency_levels {
        let files: Vec<_> = (0..n).map(|_| create_temp_file(file_size)).collect();
        let paths: Vec<_> = files.iter().map(|(_, p)| p.clone()).collect();

        group.throughput(Throughput::Bytes((file_size * n) as u64));

        group.bench_with_input(BenchmarkId::new("multi_thread", n), &paths, |b, paths| {
            let rt = rt_multi_thread();
            b.iter(|| {
                let paths = paths.clone();
                rt.block_on(async {
                    let mut set = tokio::task::JoinSet::new();
                    for path in paths {
                        set.spawn(async move { tokio::fs::read(&path).await.unwrap() });
                    }
                    while let Some(res) = set.join_next().await {
                        black_box(res.unwrap());
                    }
                })
            });
        });

        group.bench_with_input(BenchmarkId::new("current_thread", n), &paths, |b, paths| {
            let rt = rt_current_thread();
            b.iter(|| {
                let paths = paths.clone();
                rt.block_on(async {
                    let mut set = tokio::task::JoinSet::new();
                    for path in paths {
                        set.spawn(async move { tokio::fs::read(&path).await.unwrap() });
                    }
                    while let Some(res) = set.join_next().await {
                        black_box(res.unwrap());
                    }
                })
            });
        });

        // Keep temp files alive through the benchmark iteration
        drop(files);
    }

    group.finish();
}

/// Concurrent streaming reads from multiple files.
///
/// `limited_blocking` caps the blocking pool at 2 threads to show
/// how `spawn_blocking` throughput degrades under contention
fn bench_concurrent_stream(c: &mut Criterion) {
    let concurrency_levels: &[usize] = &[4, 16, 64];
    let file_size: usize = 256 * 1024 * 1024; // 256 MiB per file
    let buf_size: usize = 64 * 1024;

    let mut group = c.benchmark_group("concurrent_stream");

    for &n in concurrency_levels {
        let files: Vec<_> = (0..n).map(|_| create_temp_file(file_size)).collect();
        let paths: Vec<_> = files.iter().map(|(_, p)| p.clone()).collect();

        group.throughput(Throughput::Bytes((file_size * n) as u64));

        group.bench_with_input(BenchmarkId::new("multi_thread", n), &paths, |b, paths| {
            let rt = rt_multi_thread();
            b.iter(|| {
                let paths = paths.clone();
                rt.block_on(async {
                    let mut set = tokio::task::JoinSet::new();
                    for path in paths {
                        set.spawn(stream_read(path, buf_size));
                    }
                    while let Some(res) = set.join_next().await {
                        res.unwrap();
                    }
                })
            });
        });

        group.bench_with_input(
            BenchmarkId::new("limited_blocking", n),
            &paths,
            |b, paths| {
                let rt = rt_limited_blocking();
                b.iter(|| {
                    let paths = paths.clone();
                    rt.block_on(async {
                        let mut set = tokio::task::JoinSet::new();
                        for path in paths {
                            set.spawn(stream_read(path, buf_size));
                        }
                        while let Some(res) = set.join_next().await {
                            res.unwrap();
                        }
                    })
                });
            },
        );

        drop(files);
    }

    group.finish();
}

criterion_group!(
    file,
    bench_devzero,
    bench_sequential_read,
    bench_buffer_size,
    bench_sync_baseline,
    bench_concurrent_reads,
    bench_concurrent_stream,
);
criterion_main!(file);
