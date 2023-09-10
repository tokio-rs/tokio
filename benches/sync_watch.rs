use rand::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Notify};

use criterion::{black_box, criterion_group, criterion_main, Criterion};

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

fn contention_resubscribe(c: &mut Criterion) {
    const NTASK: u64 = 1000;

    let rt = rt();
    let (snd, rcv) = watch::channel(0i32);
    let wg = Arc::new((AtomicU64::new(0), Notify::new()));
    for n in 0..NTASK {
        let mut rcv = rcv.clone();
        let wg = wg.clone();
        let mut rng = rand::rngs::StdRng::seed_from_u64(n);
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

    c.bench_function("contention_resubscribe", |b| {
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..100 {
                    assert_eq!(wg.0.fetch_add(NTASK, Ordering::Relaxed), 0);
                    let _ = snd.send(black_box(42));
                    while wg.0.load(Ordering::Acquire) > 0 {
                        wg.1.notified().await;
                    }
                }
            });
        })
    });
}

criterion_group!(contention, contention_resubscribe);

criterion_main!(contention);
