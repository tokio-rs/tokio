use std::sync::Arc;
use tokio::{sync::Semaphore, task};

use criterion::{criterion_group, criterion_main, Criterion};

fn uncontended(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(10));
    c.bench_function("uncontended", |b| {
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

fn uncontended_concurrent_multi(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(10));
    c.bench_function("uncontended_concurrent_multi", |b| {
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

fn uncontended_concurrent_single(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(10));
    c.bench_function("uncontended_concurrent_single", |b| {
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

fn contended_concurrent_multi(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(5));
    c.bench_function("contended_concurrent_multi", |b| {
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

fn contended_concurrent_single(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(5));
    c.bench_function("contended_concurrent_single", |b| {
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

criterion_group!(
    sync_semaphore,
    uncontended,
    uncontended_concurrent_multi,
    uncontended_concurrent_single,
    contended_concurrent_multi,
    contended_concurrent_single
);

criterion_main!(sync_semaphore);
