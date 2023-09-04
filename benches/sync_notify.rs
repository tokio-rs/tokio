use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

use criterion::{criterion_group, criterion_main, Criterion};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

fn notify_waiters<const N_WAITERS: usize>(c: &mut Criterion) {
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
    c.bench_function("notify_waiters", |b| {
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

fn notify_one<const N_WAITERS: usize>(c: &mut Criterion) {
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
    c.bench_function("notify_one", |b| {
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

criterion_group!(
    notify_waiters_simple,
    notify_waiters::<10>,
    notify_waiters::<50>,
    notify_waiters::<100>,
    notify_waiters::<200>,
    notify_waiters::<500>
);

criterion_group!(
    notify_one_simple,
    notify_one::<10>,
    notify_one::<50>,
    notify_one::<100>,
    notify_one::<200>,
    notify_one::<500>
);

criterion_main!(notify_waiters_simple, notify_one_simple);
