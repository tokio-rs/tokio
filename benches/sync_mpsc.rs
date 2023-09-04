use tokio::sync::mpsc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

type Medium = [usize; 64];
type Large = [Medium; 64];

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

fn create_1_medium(c: &mut Criterion) {
    c.bench_function("create_1_medium", |b| {
        b.iter(|| {
            black_box(&mpsc::channel::<Medium>(1));
        })
    });
}

fn create_100_medium(c: &mut Criterion) {
    c.bench_function("create_100_medium", |b| {
        b.iter(|| {
            black_box(&mpsc::channel::<Medium>(100));
        })
    });
}

fn create_100_000_medium(c: &mut Criterion) {
    c.bench_function("create_100_000_medium", |b| {
        b.iter(|| {
            black_box(&mpsc::channel::<Medium>(100_000));
        })
    });
}

fn send_medium(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("send_medium", |b| {
        b.iter(|| {
            let (tx, mut rx) = mpsc::channel::<Medium>(1000);

            let _ = rt.block_on(tx.send([0; 64]));

            rt.block_on(rx.recv()).unwrap();
        })
    });
}

fn send_large(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("send_large", |b| {
        b.iter(|| {
            let (tx, mut rx) = mpsc::channel::<Large>(1000);

            let _ = rt.block_on(tx.send([[0; 64]; 64]));

            rt.block_on(rx.recv()).unwrap();
        })
    });
}

fn contention_bounded(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("contention_bounded", |b| {
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

fn contention_bounded_full(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("contention_bounded_full", |b| {
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

fn contention_unbounded(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("contention_unbounded", |b| {
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

fn uncontented_bounded(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("uncontented_bounded", |b| {
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

fn uncontented_unbounded(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("uncontented_unbounded", |b| {
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

criterion_group!(
    create,
    create_1_medium,
    create_100_medium,
    create_100_000_medium
);

criterion_group!(send, send_medium, send_large);

criterion_group!(
    contention,
    contention_bounded,
    contention_bounded_full,
    contention_unbounded,
    uncontented_bounded,
    uncontented_unbounded
);

criterion_main!(create, send, contention);
