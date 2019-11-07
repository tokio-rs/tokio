#![warn(rust_2018_idioms)]

use std::{pin::Pin, thread};

use tokio::sync::mpsc::*;

use futures::{
    executor::{block_on, block_on_stream},
    future,
    task::Poll,
};

use criterion::{black_box, criterion_group, Bencher, Criterion};

type Medium = [usize; 64];
type Large = [Medium; 64];

fn bounded_new_medium(b: &mut Bencher<'_>) {
    b.iter(|| {
        let _ = black_box(&channel::<Medium>(1_000));
    })
}

fn unbounded_new_medium(b: &mut Bencher<'_>) {
    b.iter(|| {
        let _ = black_box(&unbounded_channel::<Medium>());
    })
}

fn bounded_new_large(b: &mut Bencher<'_>) {
    b.iter(|| {
        let _ = black_box(&channel::<Large>(1_000));
    })
}

fn unbounded_new_large(b: &mut Bencher<'_>) {
    b.iter(|| {
        let _ = black_box(&unbounded_channel::<Large>());
    })
}

fn send_one_message(b: &mut Bencher<'_>) {
    block_on(future::lazy(|cx| {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1_000);

            // Send
            tx.try_send(1).unwrap();

            // Receive
            assert_eq!(Poll::Ready(Some(1)), rx.poll_recv(cx));
        })
    }))
}

fn send_one_message_large(b: &mut Bencher<'_>) {
    block_on(future::lazy(|cx| {
        b.iter(|| {
            let (mut tx, mut rx) = channel::<Large>(1_000);

            // Send
            let _ = tx.try_send([[0; 64]; 64]);

            // Receive
            let _ = black_box(&rx.poll_recv(cx));
        })
    }))
}

fn bounded_rx_not_ready(b: &mut Bencher<'_>) {
    let (_tx, mut rx) = channel::<i32>(1_000);
    b.iter(|| {
        block_on(future::lazy(|cx| {
            assert!(rx.poll_recv(cx).is_pending());
        }));
    })
}

fn bounded_tx_poll_ready(b: &mut Bencher<'_>) {
    let (mut tx, _rx) = channel::<i32>(1);
    b.iter(|| {
        block_on(future::lazy(|cx| {
            assert!(tx.poll_ready(cx).is_ready());
        }));
    })
}

fn bounded_tx_poll_not_ready(b: &mut Bencher<'_>) {
    let (mut tx, _rx) = channel::<i32>(1);
    tx.try_send(1).unwrap();
    b.iter(|| {
        block_on(future::lazy(|cx| {
            assert!(tx.poll_ready(cx).is_pending());
        }))
    })
}

fn unbounded_rx_not_ready(b: &mut Bencher<'_>) {
    let (_tx, mut rx) = unbounded_channel::<i32>();
    b.iter(|| {
        block_on(future::lazy(|cx| {
            assert!(rx.poll_recv(cx).is_pending());
        }))
    })
}

fn unbounded_rx_not_ready_x5(b: &mut Bencher<'_>) {
    let (_tx, mut rx) = unbounded_channel::<i32>();
    b.iter(|| {
        block_on(future::lazy(|cx| {
            assert!(Pin::new(&mut rx).poll_recv(cx).is_pending());
            assert!(Pin::new(&mut rx).poll_recv(cx).is_pending());
            assert!(Pin::new(&mut rx).poll_recv(cx).is_pending());
            assert!(Pin::new(&mut rx).poll_recv(cx).is_pending());
            assert!(Pin::new(&mut rx).poll_recv(cx).is_pending());
        }));
    })
}

fn bounded_uncontended_1(b: &mut Bencher<'_>) {
    block_on(future::lazy(|cx| {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1_000);

            for i in 0..1000 {
                tx.try_send(i).unwrap();
                // No need to create a task, because poll is not going to park.
                assert_eq!(Poll::Ready(Some(i)), Pin::new(&mut rx).poll_recv(cx));
            }
        })
    }))
}

fn bounded_uncontended_1_large(b: &mut Bencher<'_>) {
    block_on(future::lazy(|cx| {
        b.iter(|| {
            let (mut tx, mut rx) = channel::<Large>(1_000);

            for i in 0..1000 {
                let _ = tx.try_send([[i; 64]; 64]);
                // No need to create a task, because poll is not going to park.
                let _ = black_box(&Pin::new(&mut rx).poll_recv(cx));
            }
        })
    }))
}

fn bounded_uncontended_2(b: &mut Bencher<'_>) {
    block_on(future::lazy(|cx| {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1000);

            for i in 0..1000 {
                tx.try_send(i).unwrap();
            }

            for i in 0..1000 {
                // No need to create a task, because poll is not going to park.
                assert_eq!(Poll::Ready(Some(i)), Pin::new(&mut rx).poll_recv(cx));
            }
        })
    }))
}

fn contended_unbounded_tx(b: &mut Bencher<'_>) {
    let mut threads = vec![];
    let mut txs = vec![];

    for _ in 0..4 {
        let (tx, rx) = ::std::sync::mpsc::channel::<Sender<i32>>();
        txs.push(tx);

        threads.push(thread::spawn(move || {
            for mut tx in rx.iter() {
                for i in 0..1_000 {
                    tx.try_send(i).unwrap();
                }
            }
        }));
    }

    b.iter(|| {
        // TODO make unbounded
        let (tx, rx) = channel::<i32>(1_000_000);

        for th in &txs {
            th.send(tx.clone()).unwrap();
        }

        drop(tx);

        let rx = block_on_stream(rx).take(4 * 1_000);

        for v in rx {
            let _ = black_box(v);
        }
    });

    drop(txs);

    for th in threads {
        th.join().unwrap();
    }
}

fn contended_bounded_tx(b: &mut Bencher<'_>) {
    const THREADS: usize = 4;
    const ITERS: usize = 100;

    let mut threads = vec![];
    let mut txs = vec![];

    for _ in 0..THREADS {
        let (tx, rx) = ::std::sync::mpsc::channel::<Sender<i32>>();
        txs.push(tx);

        threads.push(thread::spawn(move || {
            block_on(async move {
                for mut tx in rx.iter() {
                    for i in 0..ITERS {
                        tx.send(i as i32).await.unwrap();
                    }
                }
            })
        }));
    }

    b.iter(|| {
        let (tx, rx) = channel::<i32>(1);

        for th in &txs {
            th.send(tx.clone()).unwrap();
        }

        drop(tx);

        let rx = block_on_stream(rx).take(THREADS * ITERS);

        for v in rx {
            let _ = black_box(v);
        }
    });

    drop(txs);

    for th in threads {
        th.join().unwrap();
    }
}

fn bench_mpsc(c: &mut Criterion) {
    c.bench_function("bounded_new_medium", bounded_new_medium);
    c.bench_function("unbounded_new_medium", unbounded_new_medium);
    c.bench_function("bounded_new_large", bounded_new_large);
    c.bench_function("unbounded_new_large", unbounded_new_large);
    c.bench_function("send_one_message", send_one_message);
    c.bench_function("send_one_message_large", send_one_message_large);
    c.bench_function("bounded_rx_not_ready", bounded_rx_not_ready);
    c.bench_function("bounded_tx_poll_ready", bounded_tx_poll_ready);
    c.bench_function("bounded_tx_poll_not_ready", bounded_tx_poll_not_ready);
    c.bench_function("unbounded_rx_not_ready", unbounded_rx_not_ready);
    c.bench_function("unbounded_rx_not_ready_x5", unbounded_rx_not_ready_x5);
    c.bench_function("bounded_uncontended_1", bounded_uncontended_1);
    c.bench_function("bounded_uncontended_1_large", bounded_uncontended_1_large);
    c.bench_function("bounded_uncontended_2", bounded_uncontended_2);
    c.bench_function("contended_unbounded_tx", contended_unbounded_tx);
    c.bench_function("contended_bounded_tx", contended_bounded_tx);
}

criterion_group!(mpsc, bench_mpsc);
fn main() {
    // The large channel tests can run out of stack
    std::thread::Builder::new()
        .stack_size(3_000_000)
        .spawn(|| {
            mpsc();

            Criterion::default().configure_from_args().final_summary();
        })
        .unwrap()
        .join()
        .unwrap();
}
