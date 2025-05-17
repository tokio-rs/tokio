#![cfg(unix)]

use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinSet;
use tokio_stream::StreamExt;

use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncReadExt;
use tokio_util::codec::{BytesCodec, FramedRead /*FramedWrite*/};

use criterion::{criterion_group, BenchmarkId};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::NamedTempFile;

use criterion::{criterion_main, Criterion};

use std::fs::File as StdFile;
use std::io::Read as StdRead;

fn rt(worker_threads: usize) -> Runtime {
    if worker_threads == 1 {
        let mut builder = Builder::new_current_thread();
        #[cfg(tokio_unstable_uring)]
        {
            builder.enable_uring();
        }
        builder.build().unwrap()
    } else {
        let mut builder = Builder::new_multi_thread();
        builder.worker_threads(worker_threads);
        #[cfg(tokio_unstable_uring)]
        {
            builder.enable_uring();
        }
        builder.build().unwrap()
    }
}

const BLOCK_COUNT: usize = 1_000;

const BUFFER_SIZE: usize = 4096;
const DEV_ZERO: &str = "/dev/zero";

fn async_read_codec(c: &mut Criterion) {
    let rt = rt(2);

    c.bench_function("async_read_codec", |b| {
        b.iter(|| {
            let task = || async {
                let file = File::open(DEV_ZERO).await.unwrap();
                let mut input_stream =
                    FramedRead::with_capacity(file, BytesCodec::new(), BUFFER_SIZE);

                for _i in 0..BLOCK_COUNT {
                    let _bytes = input_stream.next().await.unwrap();
                }
            };

            rt.block_on(task());
        })
    });
}

fn async_read_buf(c: &mut Criterion) {
    let rt = rt(2);

    c.bench_function("async_read_buf", |b| {
        b.iter(|| {
            let task = || async {
                let mut file = File::open(DEV_ZERO).await.unwrap();
                let mut buffer = [0u8; BUFFER_SIZE];

                for _i in 0..BLOCK_COUNT {
                    let count = file.read(&mut buffer).await.unwrap();
                    if count == 0 {
                        break;
                    }
                }
            };

            rt.block_on(task());
        });
    });
}

fn async_read_std_file(c: &mut Criterion) {
    let rt = rt(2);

    c.bench_function("async_read_std_file", |b| {
        b.iter(|| {
            let task = || async {
                let mut file =
                    tokio::task::block_in_place(|| Box::pin(StdFile::open(DEV_ZERO).unwrap()));

                for _i in 0..BLOCK_COUNT {
                    let mut buffer = [0u8; BUFFER_SIZE];
                    let mut file_ref = file.as_mut();

                    tokio::task::block_in_place(move || {
                        file_ref.read_exact(&mut buffer).unwrap();
                    });
                }
            };

            rt.block_on(task());
        });
    });
}

fn sync_read(c: &mut Criterion) {
    c.bench_function("sync_read", |b| {
        b.iter(|| {
            let mut file = StdFile::open(DEV_ZERO).unwrap();
            let mut buffer = [0u8; BUFFER_SIZE];

            for _i in 0..BLOCK_COUNT {
                file.read_exact(&mut buffer).unwrap();
            }
        })
    });
}

fn create_tmp_files(num_files: usize) -> (Vec<NamedTempFile>, Vec<PathBuf>) {
    let mut files = Vec::with_capacity(num_files);
    for _ in 0..num_files {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        files.push((tmp, path));
    }

    files.into_iter().unzip()
}

fn open_many_files(c: &mut Criterion) {
    const NUM_FILES: usize = 512;

    let (_tmp_files, paths): (Vec<NamedTempFile>, Vec<PathBuf>) = create_tmp_files(NUM_FILES);

    let mut group = c.benchmark_group("open_many_files");

    for &threads in &[1, 2, 4, 8, 16, 32] {
        let rt = rt(threads);

        let paths = paths.clone();
        group.bench_with_input(
            BenchmarkId::from_parameter(threads),
            &threads,
            move |b, &_threads| {
                b.iter_custom(|iter| {
                    rt.block_on(async {
                        let mut set = JoinSet::new();

                        let start = Instant::now();

                        for i in 0..(iter as usize) {
                            let path = paths.get(i % NUM_FILES).unwrap().clone();
                            set.spawn(async move {
                                let _file = OpenOptions::new().read(true).open(path).await.unwrap();
                            });
                        }
                        while let Some(Ok(_)) = set.join_next().await {}

                        start.elapsed()
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    file,
    async_read_std_file,
    async_read_buf,
    async_read_codec,
    sync_read,
    open_many_files
);
criterion_main!(file);
