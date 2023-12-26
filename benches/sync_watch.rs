use rand::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Notify};

use criterion::measurement::WallTime;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkGroup, Criterion};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

fn do_work(rng: &mut impl RngCore) -> u32 {
    use std::fmt::Write;
    let mut message = String::new();
    for i in 1..=10 {
        let _ = write!(&mut message, " {i}={}", rng.gen::<f64>());
    }
    message
        .as_bytes()
        .iter()
        .map(|&c| c as u32)
        .fold(0, u32::wrapping_add)
}

fn contention_resubscribe<const N_TASKS: usize>(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();
    let (snd, _) = watch::channel(0i32);
    let snd = Arc::new(snd);
    let wg = Arc::new((AtomicU64::new(0), Notify::new()));
    for n in 0..N_TASKS {
        let mut rcv = snd.subscribe();
        let wg = wg.clone();
        let mut rng = rand::rngs::StdRng::seed_from_u64(n as u64);
        rt.spawn(async move {
            while rcv.changed().await.is_ok() {
                let _ = *rcv.borrow(); // contend on rwlock
                let r = do_work(&mut rng);
                let _ = black_box(r);
                if wg.0.fetch_sub(1, Ordering::Release) == 1 {
                    wg.1.notify_one();
                }
            }
        });
    }

    const N_ITERS: usize = 100;
    g.bench_function(N_TASKS.to_string(), |b| {
        b.iter(|| {
            rt.block_on({
                let snd = snd.clone();
                let wg = wg.clone();
                async move {
                    tokio::spawn(async move {
                        for _ in 0..N_ITERS {
                            assert_eq!(wg.0.fetch_add(N_TASKS as u64, Ordering::Relaxed), 0);
                            let _ = snd.send(black_box(42));
                            while wg.0.load(Ordering::Acquire) > 0 {
                                wg.1.notified().await;
                            }
                        }
                    })
                    .await
                    .unwrap();
                }
            });
        })
    });
}

fn bench_contention_resubscribe(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention_resubscribe");
    contention_resubscribe::<10>(&mut group);
    contention_resubscribe::<100>(&mut group);
    contention_resubscribe::<500>(&mut group);
    contention_resubscribe::<1000>(&mut group);
    group.finish();
}

criterion_group!(contention, bench_contention_resubscribe);

criterion_main!(contention);
