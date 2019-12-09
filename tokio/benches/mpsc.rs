#![feature(test)]

extern crate test;

use tokio::sync::mpsc::*;

use futures::{future, task, StreamExt};
use std::future::Future;
use std::task::{Context, Poll};
use std::thread;
use test::Bencher;

type Medium = [usize; 64];
type Large = [Medium; 64];

#[bench]
fn bounded_new_medium(b: &mut Bencher) {
    b.iter(|| {
        let _ = test::black_box(&channel::<Medium>(1_000));
    })
}

#[bench]
fn unbounded_new_medium(b: &mut Bencher) {
    b.iter(|| {
        let _ = test::black_box(&unbounded_channel::<Medium>());
    })
}
#[bench]
fn bounded_new_large(b: &mut Bencher) {
    b.iter(|| {
        let _ = test::black_box(&channel::<Large>(1_000));
    })
}

#[bench]
fn unbounded_new_large(b: &mut Bencher) {
    b.iter(|| {
        let _ = test::black_box(&unbounded_channel::<Large>());
    })
}

#[bench]
fn send_one_message(b: &mut Bencher) {
    b.iter(|| {
        let (mut tx, mut rx) = channel(1_000);

        // Send
        tx.try_send(1).unwrap();
        let waker = task::noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        // Receive
        assert_eq!(Poll::Ready(Some(1)), rx.poll_recv(&mut cx));
    })
}

#[bench]
fn send_one_message_large(b: &mut Bencher) {
    b.iter(|| {
        let (mut tx, mut rx) = channel::<Large>(1_000);

        // Send
        let _ = tx.try_send([[0; 64]; 64]);
        let waker = task::noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        // Receive
        let _ = test::black_box(&rx.poll_recv(&mut cx));
    })
}

#[bench]
fn bounded_rx_not_ready(b: &mut Bencher) {
    let (_tx, mut rx) = channel::<i32>(1_000);
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let fut = future::lazy(|mut cx| {
            assert!(rx.poll_recv(&mut cx).is_pending());

            Ok::<_, ()>(())
        });
        futures::pin_mut!(fut);
        loop {
            if let Poll::Ready(_) = fut.as_mut().poll(&mut cx) {
                break;
            }
        }
    })
}

#[bench]
fn bounded_tx_poll_ready(b: &mut Bencher) {
    let (mut tx, _rx) = channel::<i32>(1);
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let fut = future::lazy(|cx| {
            assert!(tx.poll_ready(cx).is_ready());

            Ok::<_, ()>(())
        });
        futures::pin_mut!(fut);
        loop {
            if let Poll::Ready(_) = fut.as_mut().poll(&mut cx) {
                break;
            }
        }
    })
}

#[bench]
fn bounded_tx_poll_not_ready(b: &mut Bencher) {
    let (mut tx, _rx) = channel::<i32>(1);
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    tx.try_send(1).unwrap();
    b.iter(|| {
        let fut = future::lazy(|cx| {
            assert!(tx.poll_ready(cx).is_pending());

            Ok::<_, ()>(())
        });

        futures::pin_mut!(fut);
        loop {
            if let Poll::Ready(_) = fut.as_mut().poll(&mut cx) {
                break;
            }
        }
    })
}

#[bench]
fn unbounded_rx_not_ready(b: &mut Bencher) {
    let (_tx, mut rx) = unbounded_channel::<i32>();
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let fut = future::lazy(|cx| {
            assert!(rx.poll_recv(cx).is_pending());

            Ok::<_, ()>(())
        });

        futures::pin_mut!(fut);
        loop {
            if let Poll::Ready(_) = fut.as_mut().poll(&mut cx) {
                break;
            }
        }
    })
}

#[bench]
fn unbounded_rx_not_ready_x5(b: &mut Bencher) {
    let (_tx, mut rx) = unbounded_channel::<i32>();
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let fut = future::lazy(|mut cx| {
            assert!(rx.poll_recv(&mut cx).is_pending());
            assert!(rx.poll_recv(&mut cx).is_pending());
            assert!(rx.poll_recv(&mut cx).is_pending());
            assert!(rx.poll_recv(&mut cx).is_pending());
            assert!(rx.poll_recv(&mut cx).is_pending());

            Ok::<_, ()>(())
        });

        futures::pin_mut!(fut);
        loop {
            if let Poll::Ready(_) = fut.as_mut().poll(&mut cx) {
                break;
            }
        }
    })
}

#[bench]
fn bounded_uncontended_1(b: &mut Bencher) {
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let (mut tx, mut rx) = channel(1_000);

        for i in 0..1000 {
            tx.try_send(i).unwrap();
            // No need to create a task, because poll is not going to park.
            assert_eq!(Poll::Ready(Some(i)), rx.poll_recv(&mut cx));
        }
    })
}

#[bench]
fn bounded_uncontended_1_large(b: &mut Bencher) {
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let (mut tx, mut rx) = channel::<Large>(1_000);

        for i in 0..1000 {
            let _ = tx.try_send([[i; 64]; 64]);
            // No need to create a task, because poll is not going to park.
            let _ = test::black_box(&rx.poll_recv(&mut cx));
        }
    })
}

#[bench]
fn bounded_uncontended_2(b: &mut Bencher) {
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    b.iter(|| {
        let (mut tx, mut rx) = channel(1000);

        for i in 0..1000 {
            tx.try_send(i).unwrap();
        }

        for i in 0..1000 {
            // No need to create a task, because poll is not going to park.
            assert_eq!(Poll::Ready(Some(i)), rx.poll_recv(&mut cx));
        }
    })
}

#[bench]
fn contended_unbounded_tx(b: &mut Bencher) {
    let mut threads = vec![];
    let mut txs = vec![];

    for _ in 0..4 {
        let (tx, rx) = ::std::sync::mpsc::channel::<UnboundedSender<i32>>();
        txs.push(tx);

        threads.push(thread::spawn(move || {
            for tx in rx.iter() {
                for i in 0..1_000 {
                    tx.send(i).unwrap();
                }
            }
        }));
    }

    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);

    b.iter(|| {
        let (tx, rx) = unbounded_channel::<i32>();

        for th in &txs {
            th.send(tx.clone()).unwrap();
        }

        drop(tx);

        let rx_fut = rx.take(4 * 1_000).collect::<Vec<_>>();
        futures::pin_mut!(rx_fut);
        loop {
            if let Poll::Ready(result) = rx_fut.as_mut().poll(&mut cx) {
                for v in result {
                    let _ = test::black_box(v);
                }
                break;
            }
        }
    });

    drop(txs);

    for th in threads {
        th.join().unwrap();
    }
}

#[bench]
fn contended_bounded_tx(b: &mut Bencher) {
    const THREADS: usize = 4;
    const ITERS: usize = 100;

    let mut threads = vec![];
    let mut txs = vec![];

    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);

    for _ in 0..THREADS {
        let (tx, rx) = ::std::sync::mpsc::channel::<Sender<i32>>();
        txs.push(tx);

        let waker = task::noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        threads.push(thread::spawn(move || {
            for mut tx in rx.iter() {
                loop {
                    if let Poll::Ready(ready) = tx.poll_ready(&mut cx) {
                        ready.expect("tx dropped");
                        for i in 0..ITERS {
                            let fut = tx.send(i as i32);
                            futures::pin_mut!(fut);
                            while let Poll::Pending = fut.as_mut().poll(&mut cx) {}
                        }
                        break;
                    }
                }
            }
        }));
    }

    b.iter(|| {
        let (tx, rx) = channel::<i32>(1);

        for th in &txs {
            th.send(tx.clone()).unwrap();
        }

        drop(tx);

        let rx = rx.take(THREADS * ITERS).collect::<Vec<_>>();
        futures::pin_mut!(rx);
        loop {
            if let Poll::Ready(result) = rx.as_mut().poll(&mut cx) {
                for v in result {
                    let _ = test::black_box(v);
                }
                break;
            }
        }
    });

    drop(txs);

    for th in threads {
        th.join().unwrap();
    }
}
