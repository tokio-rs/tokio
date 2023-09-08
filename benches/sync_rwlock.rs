use std::sync::Arc;
use tokio::{sync::RwLock, task};

use criterion::measurement::WallTime;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkGroup, Criterion};

fn read_uncontended(g: &mut BenchmarkGroup<WallTime>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    let lock = Arc::new(RwLock::new(()));
    g.bench_function("read", |b| {
        b.iter(|| {
            let lock = lock.clone();
            rt.block_on(async move {
                for _ in 0..6 {
                    let read = lock.read().await;
                    let _read = black_box(read);
                }
            })
        })
    });
}

fn read_concurrent_uncontended_multi(g: &mut BenchmarkGroup<WallTime>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    g.bench_function("read_concurrent_multi", |b| {
        b.iter(|| {
            let lock = lock.clone();
            rt.block_on(async move {
                let j = tokio::try_join! {
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone()))
                };
                j.unwrap();
            })
        })
    });
}

fn read_concurrent_uncontended(g: &mut BenchmarkGroup<WallTime>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    g.bench_function("read_concurrent", |b| {
        b.iter(|| {
            let lock = lock.clone();
            rt.block_on(async move {
                tokio::join! {
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone())
                };
            })
        })
    });
}

fn read_concurrent_contended_multi(g: &mut BenchmarkGroup<WallTime>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    g.bench_function("read_concurrent_multi", |b| {
        b.iter(|| {
            let lock = lock.clone();
            rt.block_on(async move {
                let write = lock.write().await;
                let j = tokio::try_join! {
                    async move { drop(write); Ok(()) },
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                    task::spawn(task(lock.clone())),
                };
                j.unwrap();
            })
        })
    });
}

fn read_concurrent_contended(g: &mut BenchmarkGroup<WallTime>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    g.bench_function("read_concurrent", |b| {
        b.iter(|| {
            let lock = lock.clone();
            rt.block_on(async move {
                let write = lock.write().await;
                tokio::join! {
                    async move { drop(write) },
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone()),
                    task(lock.clone()),
                };
            })
        })
    });
}

fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    read_concurrent_contended(&mut group);
    read_concurrent_contended_multi(&mut group);
    group.finish();
}

fn bench_uncontented(c: &mut Criterion) {
    let mut group = c.benchmark_group("uncontented");
    read_uncontended(&mut group);
    read_concurrent_uncontended(&mut group);
    read_concurrent_uncontended_multi(&mut group);
    group.finish();
}

criterion_group!(contention, bench_contention);
criterion_group!(uncontented, bench_uncontented);

criterion_main!(contention, uncontented);
