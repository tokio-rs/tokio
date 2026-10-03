use rand::{Rng, RngCore, SeedableRng};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, Notify};

use criterion::measurement::WallTime;
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkGroup, Criterion, Throughput,
};

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

fn bench_try_recv(c: &mut Criterion) {
    let mut group = c.benchmark_group("try_recv");
    const MESSAGES: usize = 256;

    for receiver_count in [1usize, 4, 16] {
        group.throughput(Throughput::Elements((MESSAGES * receiver_count) as u64));
        group.bench_function(receiver_count.to_string(), |b| {
            let (tx, first_rx) = broadcast::channel::<usize>(MESSAGES);
            let mut receivers = Vec::with_capacity(receiver_count);
            receivers.push(first_rx);

            for _ in 1..receiver_count {
                receivers.push(tx.subscribe());
            }

            b.iter(|| {
                for message in 0..MESSAGES {
                    tx.send(black_box(message)).unwrap();
                }

                for rx in &mut receivers {
                    for _ in 0..MESSAGES {
                        black_box(rx.try_recv().unwrap());
                    }
                }
            });
        });
    }

    group.finish();
}

/// Several senders sending concurrently while many receivers keep queuing
/// themselves for the next value.
fn contention_multi_tx_impl<const N_RX: usize>(g: &mut BenchmarkGroup<WallTime>) {
    const N_TX: usize = 4;
    const N_MSGS: usize = 1000;

    let rt = rt();

    let (tx, _rx) = broadcast::channel::<usize>(1024);

    for _ in 0..N_RX {
        let mut rx = tx.subscribe();
        rt.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(v) => {
                        black_box(v);
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    g.throughput(Throughput::Elements((N_TX * N_MSGS) as u64));
    g.bench_function(N_RX.to_string(), |b| {
        b.iter(|| {
            rt.block_on(async {
                let senders: Vec<_> = (0..N_TX)
                    .map(|_| {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            for i in 0..N_MSGS {
                                tx.send(i).unwrap();
                            }
                        })
                    })
                    .collect();

                for sender in senders {
                    sender.await.unwrap();
                }
            })
        })
    });
}

fn bench_contention_multi_tx(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention_multi_tx");
    contention_multi_tx_impl::<10>(&mut group);
    contention_multi_tx_impl::<100>(&mut group);
    contention_multi_tx_impl::<1000>(&mut group);
    group.finish();
}

criterion_group!(
    contention,
    bench_contention,
    bench_contention_multi_tx,
    bench_try_recv
);

criterion_main!(contention);
