//! Benchmark spawning a task onto the basic and threaded Tokio executors.
//! This essentially measure the time to enqueue a task in the local and remote
//! case.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

async fn work() -> usize {
    let val = 1 + 1;
    tokio::task::yield_now().await;
    black_box(val)
}

fn basic_scheduler_spawn(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    c.bench_function("basic_scheduler_spawn", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let h = tokio::spawn(work());
                assert_eq!(h.await.unwrap(), 2);
            });
        })
    });
}

fn basic_scheduler_spawn10(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    c.bench_function("basic_scheduler_spawn10", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let mut handles = Vec::with_capacity(10);
                for _ in 0..10 {
                    handles.push(tokio::spawn(work()));
                }
                for handle in handles {
                    assert_eq!(handle.await.unwrap(), 2);
                }
            });
        })
    });
}

fn threaded_scheduler_spawn(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();
    c.bench_function("threaded_scheduler_spawn", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let h = tokio::spawn(work());
                assert_eq!(h.await.unwrap(), 2);
            });
        })
    });
}

fn threaded_scheduler_spawn10(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();
    c.bench_function("threaded_scheduler_spawn10", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let mut handles = Vec::with_capacity(10);
                for _ in 0..10 {
                    handles.push(tokio::spawn(work()));
                }
                for handle in handles {
                    assert_eq!(handle.await.unwrap(), 2);
                }
            });
        })
    });
}

criterion_group!(
    spawn,
    basic_scheduler_spawn,
    basic_scheduler_spawn10,
    threaded_scheduler_spawn,
    threaded_scheduler_spawn10,
);

criterion_main!(spawn);
