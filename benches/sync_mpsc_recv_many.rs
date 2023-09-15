use bencher::Bencher;
use tokio::sync::mpsc;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

// Simulate a use case of an actor that must update
// a resource, but resource only needs last value
fn publish_last_value(last_value: usize) -> usize {
    std::thread::sleep(std::time::Duration::from_nanos(1));
    last_value
}

fn contention_bounded_updater_recv(b: &mut Bencher) {
    let rt = rt();

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

            let mut last_value = 0usize;
            for _ in 0..1_000 * 5 {
                let Some(v) = rx.recv().await else {continue};
                last_value = v
            }
            last_value
        })
    });
}

fn contention_bounded_updater_recv_many(b: &mut Bencher) {
    let rt = rt();

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

            let mut last_value = 0usize;
            let mut buffer = Vec::<usize>::with_capacity(5_000);
            let mut total = 0;
            while total < 1_000 * 5 {
                let count = rx.recv_many(&mut buffer).await;
                total += count;
                if count > 0 {
                    last_value = buffer[buffer.len() - 1]
                }
            }
            last_value
        })
    });
}

fn contention_bounded_updater_publish_recv(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        rt.block_on(async move {
            let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

            for _ in 0..1 {
                let tx = tx.clone();
                tokio::spawn(async move {
                    for i in 0..1000 {
                        tx.send(i).await.unwrap();
                    }
                });
            }

            for _ in 0..1_000 {
                let Some(v) = rx.recv().await else {continue};
                let _ = publish_last_value(v);
            }
        })
    });
}

fn contention_bounded_updater_publish_recv_many(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        rt.block_on(async move {
            let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

            for _ in 0..1 {
                let tx = tx.clone();
                tokio::spawn(async move {
                    for i in 0..1000 {
                        tx.send(i).await.unwrap();
                    }
                });
            }

            let mut buffer = Vec::<usize>::with_capacity(5_000);
            let mut total = 0;
            while total < 1_000 * 1 {
                let count = rx.recv_many(&mut buffer).await;
                total += count;
                if count > 0 {
                    publish_last_value(buffer[buffer.len() - 1]);
                }
            }
        })
    });
}

fn contention_bounded_full_updater_recv(b: &mut Bencher) {
    let rt = rt();

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

            let mut last_value = 0usize;
            for _ in 0..1_000 * 5 {
                let Some(v) = rx.recv().await else {continue};
                last_value = v
            }
            last_value
        })
    });
}

fn contention_bounded_full_updater_recv_many(b: &mut Bencher) {
    let rt = rt();

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

            let mut last_value = 0usize;
            let mut buffer = Vec::<usize>::with_capacity(5_000);
            let mut total = 0;
            while total < 1_000 * 5 {
                let count = rx.recv_many(&mut buffer).await;
                total += count;
                if count > 0 {
                    last_value = buffer[buffer.len() - 1]
                }
            }
            last_value
        })
    });
}

fn contention_unbounded_updater_recv(b: &mut Bencher) {
    let rt = rt();

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

            let mut last_value = 0usize;
            for _ in 0..1_000 * 5 {
                let Some(v) = rx.recv().await else {continue};
                last_value = v
            }
            last_value
        })
    });
}

fn contention_unbounded_updater_recv_many(b: &mut Bencher) {
    let rt = rt();

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

            let mut last_value = 0usize;
            let mut buffer = Vec::<usize>::with_capacity(5_000);
            let mut total = 0;
            while total < 1_000 * 5 {
                let count = rx.recv_many(&mut buffer).await;
                total += count;
                if count > 0 {
                    last_value = buffer[buffer.len() - 1]
                }
            }
            last_value
        })
    });
}

fn uncontented_bounded_updater_recv(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        rt.block_on(async move {
            let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

            for i in 0..5000 {
                tx.send(i).await.unwrap();
            }

            let mut last_value = 0usize;
            for _ in 0..5_000 {
                let Some(v) = rx.recv().await else {continue};
                last_value = v
            }
            last_value
        })
    });
}

fn uncontented_bounded_updater_recv_many(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        rt.block_on(async move {
            let (tx, mut rx) = mpsc::channel::<usize>(1_000_000);

            for i in 0..5000 {
                tx.send(i).await.unwrap();
            }

            let mut last_value = 0usize;
            let mut buffer = Vec::<usize>::with_capacity(5_000);
            let mut total = 0;
            while total < 1_000 * 5 {
                let count = rx.recv_many(&mut buffer).await;
                total += count;
                if count > 0 {
                    last_value = buffer[buffer.len() - 1]
                }
            }
            last_value
        })
    });
}

fn uncontented_unbounded_updater_recv(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        rt.block_on(async move {
            let (tx, mut rx) = mpsc::unbounded_channel::<usize>();

            for i in 0..5000 {
                tx.send(i).unwrap();
            }

            let mut last_value = 0usize;
            for _ in 0..5_000 {
                let Some(v) = rx.recv().await else {continue};
                last_value = v
            }
            last_value
        })
    });
}

fn uncontented_unbounded_updater_recv_many(b: &mut Bencher) {
    let rt = rt();

    b.iter(|| {
        rt.block_on(async move {
            let (tx, mut rx) = mpsc::unbounded_channel::<usize>();

            for i in 0..5000 {
                tx.send(i).unwrap();
            }

            let mut last_value = 0usize;
            let mut buffer = Vec::<usize>::with_capacity(5_000);
            let mut total = 0;
            while total < 1_000 * 5 {
                let count = rx.recv_many(&mut buffer).await;
                total += count;
                if count > 0 {
                    last_value = buffer[buffer.len() - 1]
                }
            }
            last_value
        })
    });
}

bencher::benchmark_group!(
    contention_bounded_updater,
    contention_bounded_updater_recv,
    contention_bounded_updater_recv_many
);

bencher::benchmark_group!(
    contention_bounded_updater_publish,
    contention_bounded_updater_publish_recv,
    contention_bounded_updater_publish_recv_many
);

bencher::benchmark_group!(
    contention_bounded_full_updater,
    contention_bounded_full_updater_recv,
    contention_bounded_full_updater_recv_many
);

bencher::benchmark_group!(
    contention_unbounded_updater,
    contention_unbounded_updater_recv,
    contention_unbounded_updater_recv_many
);

bencher::benchmark_group!(
    uncontented_bounded_updater,
    uncontented_bounded_updater_recv,
    uncontented_bounded_updater_recv_many
);

bencher::benchmark_group!(
    uncontented_unbounded_updater,
    uncontented_unbounded_updater_recv,
    uncontented_unbounded_updater_recv_many
);

bencher::benchmark_main!(
    contention_bounded_updater,
    contention_bounded_updater_publish,
    contention_bounded_full_updater,
    contention_unbounded_updater,
    uncontented_bounded_updater,
    uncontented_unbounded_updater
);
