use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

fn notify_waiters<const N_WAITERS: usize>(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();
    let notify = Arc::new(Notify::new());
    let counter = Arc::new(AtomicUsize::new(0));
    for _ in 0..N_WAITERS {
        rt.spawn({
            let notify = notify.clone();
            let counter = counter.clone();
            async move {
                loop {
                    notify.notified().await;
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    const N_ITERS: usize = 500;
    g.bench_function(N_WAITERS.to_string(), |b| {
        b.iter(|| {
            counter.store(0, Ordering::Relaxed);
            loop {
                notify.notify_waiters();
                if counter.load(Ordering::Relaxed) >= N_ITERS {
                    break;
                }
            }
        })
    });
}

fn notify_one<const N_WAITERS: usize>(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();
    let notify = Arc::new(Notify::new());
    let counter = Arc::new(AtomicUsize::new(0));
    for _ in 0..N_WAITERS {
        rt.spawn({
            let notify = notify.clone();
            let counter = counter.clone();
            async move {
                loop {
                    notify.notified().await;
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    const N_ITERS: usize = 500;
    g.bench_function(N_WAITERS.to_string(), |b| {
        b.iter(|| {
            counter.store(0, Ordering::Relaxed);
            loop {
                notify.notify_one();
                if counter.load(Ordering::Relaxed) >= N_ITERS {
                    break;
                }
            }
        })
    });
}

fn bench_notify_one(c: &mut Criterion) {
    let mut group = c.benchmark_group("notify_one");
    notify_one::<10>(&mut group);
    notify_one::<50>(&mut group);
    notify_one::<100>(&mut group);
    notify_one::<200>(&mut group);
    notify_one::<500>(&mut group);
    group.finish();
}

fn bench_notify_waiters(c: &mut Criterion) {
    let mut group = c.benchmark_group("notify_waiters");
    notify_waiters::<10>(&mut group);
    notify_waiters::<50>(&mut group);
    notify_waiters::<100>(&mut group);
    notify_waiters::<200>(&mut group);
    notify_waiters::<500>(&mut group);
    group.finish();
}

criterion_group!(
    notify_waiters_simple,
    bench_notify_one,
    bench_notify_waiters
);

criterion_main!(notify_waiters_simple);
