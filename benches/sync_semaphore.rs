use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::{sync::Semaphore, task};

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};

fn single_rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}

fn multi_rt() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

fn uncontended(g: &mut BenchmarkGroup<WallTime>) {
    let rt = multi_rt();

    let s = Arc::new(Semaphore::new(10));
    g.bench_function("multi", |b| {
        b.iter(|| {
            let s = s.clone();
            rt.block_on(async move {
                for _ in 0..6 {
                    let permit = s.acquire().await;
                    drop(permit);
                }
            })
        })
    });
}

async fn task(s: Arc<Semaphore>) {
    let permit = s.acquire().await;
    drop(permit);
}

fn uncontended_concurrent_multi(g: &mut BenchmarkGroup<WallTime>) {
    let rt = multi_rt();

    let s = Arc::new(Semaphore::new(10));
    g.bench_function("concurrent_multi", |b| {
        b.iter(|| {
            let s = s.clone();
            rt.block_on(async move {
                let j = tokio::try_join! {
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone()))
                };
                j.unwrap();
            })
        })
    });
}

fn uncontended_concurrent_single(g: &mut BenchmarkGroup<WallTime>) {
    let rt = single_rt();

    let s = Arc::new(Semaphore::new(10));
    g.bench_function("concurrent_single", |b| {
        b.iter(|| {
            let s = s.clone();
            rt.block_on(async move {
                tokio::join! {
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone())
                };
            })
        })
    });
}

fn contended_concurrent_multi(g: &mut BenchmarkGroup<WallTime>) {
    let rt = multi_rt();

    let s = Arc::new(Semaphore::new(5));
    g.bench_function("concurrent_multi", |b| {
        b.iter(|| {
            let s = s.clone();
            rt.block_on(async move {
                let j = tokio::try_join! {
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone())),
                    task::spawn(task(s.clone()))
                };
                j.unwrap();
            })
        })
    });
}

fn contended_concurrent_single(g: &mut BenchmarkGroup<WallTime>) {
    let rt = single_rt();

    let s = Arc::new(Semaphore::new(5));
    g.bench_function("concurrent_single", |b| {
        b.iter(|| {
            let s = s.clone();
            rt.block_on(async move {
                tokio::join! {
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone()),
                    task(s.clone())
                };
            })
        })
    });
}

fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    contended_concurrent_multi(&mut group);
    contended_concurrent_single(&mut group);
    group.finish();
}

fn bench_uncontented(c: &mut Criterion) {
    let mut group = c.benchmark_group("uncontented");
    uncontended(&mut group);
    uncontended_concurrent_multi(&mut group);
    uncontended_concurrent_single(&mut group);
    group.finish();
}

criterion_group!(contention, bench_contention);
criterion_group!(uncontented, bench_uncontented);

criterion_main!(contention, uncontented);
