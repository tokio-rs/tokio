use tokio::sync::mpsc;

use criterion::measurement::WallTime;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkGroup, Criterion};

#[derive(Debug, Copy, Clone)]
struct Medium(#[allow(dead_code)] [usize; 64]);
impl Default for Medium {
    fn default() -> Self {
        Medium([0; 64])
    }
}

#[derive(Debug, Copy, Clone)]
struct Large(#[allow(dead_code)] [Medium; 64]);
impl Default for Large {
    fn default() -> Self {
        Large([Medium::default(); 64])
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

fn create_medium<const SIZE: usize>(g: &mut BenchmarkGroup<WallTime>) {
    g.bench_function(SIZE.to_string(), |b| {
        b.iter(|| {
            black_box(&mpsc::channel::<Medium>(SIZE));
        })
    });
}

fn send_data<T: Default, const SIZE: usize>(g: &mut BenchmarkGroup<WallTime>, prefix: &str) {
    let rt = rt();

    g.bench_function(format!("{}_{}", prefix, SIZE), |b| {
        b.iter(|| {
            let (tx, mut rx) = mpsc::channel::<T>(SIZE);

            let _ = rt.block_on(tx.send(T::default()));

            rt.block_on(rx.recv()).unwrap();
        })
    });
}

fn contention_bounded(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("bounded", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

                for _ in 0..5 {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for i in 0..1000 {
                            tx.send(i).await.unwrap();
                        }
                    });
                }

                for _ in 0..1_000 * 5 {
                    let _ = rx.recv().await;
                }
            })
        })
    });
}

fn contention_bounded_recv_many(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("bounded_recv_many", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

                for _ in 0..5 {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for i in 0..1000 {
                            tx.send(i).await.unwrap();
                        }
                    });
                }

                let mut buffer = Vec::<usize>::with_capacity(5_000);
                let mut total = 0;
                while total < 1_000 * 5 {
                    total += rx.recv_many(&mut buffer, 5_000).await;
                }
            })
        })
    });
}

fn contention_bounded_full(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("bounded_full", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::channel::<usize>(100);

                for _ in 0..5 {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for i in 0..1000 {
                            tx.send(i).await.unwrap();
                        }
                    });
                }

                for _ in 0..1_000 * 5 {
                    let _ = rx.recv().await;
                }
            })
        })
    });
}

fn contention_bounded_full_recv_many(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("bounded_full_recv_many", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::channel::<usize>(100);

                for _ in 0..5 {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for i in 0..1000 {
                            tx.send(i).await.unwrap();
                        }
                    });
                }

                let mut buffer = Vec::<usize>::with_capacity(5_000);
                let mut total = 0;
                while total < 1_000 * 5 {
                    total += rx.recv_many(&mut buffer, 5_000).await;
                }
            })
        })
    });
}

fn contention_unbounded(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("unbounded", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::unbounded_channel::<usize>();

                for _ in 0..5 {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for i in 0..1000 {
                            tx.send(i).unwrap();
                        }
                    });
                }

                for _ in 0..1_000 * 5 {
                    let _ = rx.recv().await;
                }
            })
        })
    });
}

fn contention_unbounded_recv_many(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("unbounded_recv_many", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::unbounded_channel::<usize>();

                for _ in 0..5 {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for i in 0..1000 {
                            tx.send(i).unwrap();
                        }
                    });
                }

                let mut buffer = Vec::<usize>::with_capacity(5_000);
                let mut total = 0;
                while total < 1_000 * 5 {
                    total += rx.recv_many(&mut buffer, 5_000).await;
                }
            })
        })
    });
}

fn uncontented_bounded(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("bounded", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

                for i in 0..5000 {
                    tx.send(i).await.unwrap();
                }

                for _ in 0..5_000 {
                    let _ = rx.recv().await;
                }
            })
        })
    });
}

fn uncontented_bounded_recv_many(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("bounded_recv_many", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

                for i in 0..5000 {
                    tx.send(i).await.unwrap();
                }

                let mut buffer = Vec::<usize>::with_capacity(5_000);
                let mut total = 0;
                while total < 1_000 * 5 {
                    total += rx.recv_many(&mut buffer, 5_000).await;
                }
            })
        })
    });
}

fn uncontented_unbounded(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("unbounded", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::unbounded_channel::<usize>();

                for i in 0..5000 {
                    tx.send(i).unwrap();
                }

                for _ in 0..5_000 {
                    let _ = rx.recv().await;
                }
            })
        })
    });
}

fn uncontented_unbounded_recv_many(g: &mut BenchmarkGroup<WallTime>) {
    let rt = rt();

    g.bench_function("unbounded_recv_many", |b| {
        b.iter(|| {
            rt.block_on(async move {
                let (tx, mut rx) = mpsc::unbounded_channel::<usize>();

                for i in 0..5000 {
                    tx.send(i).unwrap();
                }

                let mut buffer = Vec::<usize>::with_capacity(5_000);
                let mut total = 0;
                while total < 1_000 * 5 {
                    total += rx.recv_many(&mut buffer, 5_000).await;
                }
            })
        })
    });
}

fn bench_create_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("create_medium");
    create_medium::<1>(&mut group);
    create_medium::<100>(&mut group);
    create_medium::<100_000>(&mut group);
    group.finish();
}

fn bench_send(c: &mut Criterion) {
    let mut group = c.benchmark_group("send");
    send_data::<Medium, 1000>(&mut group, "medium");
    send_data::<Large, 1000>(&mut group, "large");
    group.finish();
}

fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    contention_bounded(&mut group);
    contention_bounded_recv_many(&mut group);
    contention_bounded_full(&mut group);
    contention_bounded_full_recv_many(&mut group);
    contention_unbounded(&mut group);
    contention_unbounded_recv_many(&mut group);
    group.finish();
}

fn bench_uncontented(c: &mut Criterion) {
    let mut group = c.benchmark_group("uncontented");
    uncontented_bounded(&mut group);
    uncontented_bounded_recv_many(&mut group);
    uncontented_unbounded(&mut group);
    uncontented_unbounded_recv_many(&mut group);
    group.finish();
}

criterion_group!(create, bench_create_medium);
criterion_group!(send, bench_send);
criterion_group!(contention, bench_contention);
criterion_group!(uncontented, bench_uncontented);

criterion_main!(create, send, contention, uncontented);
