use rand::{Rng, RngCore, SeedableRng};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Notify};

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
        let _ = write!(&mut message, " {i}={}", rng.random::<f64>());
    }
    message
        .as_bytes()
        .iter()
        .map(|&c| c as u32)
        .fold(0, u32::wrapping_add)
}

fn contention_impl<const N_TASKS: usize>(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    let (tx, _rx) = broadcast::channel::<usize>(1000);
    let wg = Arc::new((AtomicUsize::new(0), Notify::new()));

    for n in 0..N_TASKS {
        let wg = wg.clone();
        let mut rx = tx.subscribe();
        let mut rng = rand::rngs::StdRng::seed_from_u64(n as u64);
        rt.spawn(async move {
            while (rx.recv().await).is_ok() {
                let r = do_work(&mut rng);
                let _ = black_box(r);
                if wg.0.fetch_sub(1, Ordering::Relaxed) == 1 {
                    wg.1.notify_one();
                }
            }
        });
    }

    const N_ITERS: usize = 100;

    g.bench_function(N_TASKS.to_string(), |b| {
        b.iter(|| {
            rt.block_on({
                let wg = wg.clone();
                let tx = tx.clone();
                async move {
                    for i in 0..N_ITERS {
                        assert_eq!(wg.0.fetch_add(N_TASKS, Ordering::Relaxed), 0);
                        tx.send(i).unwrap();
                        while wg.0.load(Ordering::Relaxed) > 0 {
                            wg.1.notified().await;
                        }
                    }
                }
            })
        })
    });
}

fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    contention_impl::<10>(&mut group);
    contention_impl::<100>(&mut group);
    contention_impl::<500>(&mut group);
    contention_impl::<1000>(&mut group);
    group.finish();
}

criterion_group!(contention, bench_contention);

criterion_main!(contention);
