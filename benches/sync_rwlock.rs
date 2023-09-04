use std::sync::Arc;
use tokio::{sync::RwLock, task};

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn read_uncontended(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    let lock = Arc::new(RwLock::new(()));
    c.bench_function("read_uncontended", |b| {
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

fn read_concurrent_uncontended_multi(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    c.bench_function("read_concurrent_uncontended_multi", |b| {
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

fn read_concurrent_uncontended(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    c.bench_function("read_concurrent_uncontended", |b| {
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

fn read_concurrent_contended_multi(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    c.bench_function("read_concurrent_contended_multi", |b| {
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

fn read_concurrent_contended(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        let _read = black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    c.bench_function("read_concurrent_contended", |b| {
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

criterion_group!(
    sync_rwlock,
    read_uncontended,
    read_concurrent_uncontended,
    read_concurrent_uncontended_multi,
    read_concurrent_contended,
    read_concurrent_contended_multi
);

criterion_main!(sync_rwlock);
