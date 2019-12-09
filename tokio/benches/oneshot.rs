#![feature(test)]

extern crate test;

use tokio::sync::oneshot;

use futures::{future, task, Future};
use std::task::{Context, Poll};
use test::Bencher;

fn spin<F: Future>(f: F) -> F::Output {
    let waker = task::noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    futures::pin_mut!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,

            Poll::Pending => {}
        }
    }
}

#[bench]
fn new(b: &mut Bencher) {
    b.iter(|| {
        let _ = ::test::black_box(&oneshot::channel::<i32>());
    })
}

#[bench]
fn same_thread_send_recv(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = oneshot::channel();

        let _ = tx.send(1);
        let waker = task::noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        futures::pin_mut!(rx);

        assert_eq!(Poll::Ready(1), rx.poll(&mut cx).map(Result::unwrap));
    });
}

#[bench]
fn same_thread_recv_multi_send_recv(b: &mut Bencher) {
    b.iter(|| {
        let (tx, rx) = oneshot::channel();
        futures::pin_mut!(rx);
        let fut = future::lazy(|mut cx| {
            let _ = rx.as_mut().poll(&mut cx);
            let _ = rx.as_mut().poll(&mut cx);
            let _ = rx.as_mut().poll(&mut cx);
            let _ = rx.as_mut().poll(&mut cx);

            let _ = tx.send(1);

            assert_eq!(Poll::Ready(1), rx.poll(&mut cx).map(Result::unwrap));

            Ok::<_, ()>(())
        });
        spin(fut).unwrap();
    });
}

#[bench]
fn multi_thread_send_recv(b: &mut Bencher) {
    const MAX: usize = 10_000_000;

    use std::thread;

    let mut ping_txs = vec![];
    let mut ping_rxs = vec![];
    let mut pong_txs = vec![];
    let mut pong_rxs = vec![];

    for _ in 0..MAX {
        let (tx, rx) = oneshot::channel::<()>();

        ping_txs.push(Some(tx));
        ping_rxs.push(Some(rx));

        let (tx, rx) = oneshot::channel::<()>();

        pong_txs.push(Some(tx));
        pong_rxs.push(Some(rx));
    }

    thread::spawn(move || {
        let fut = future::lazy(|_| {
            for i in 0..MAX {
                let ping_rx = ping_rxs[i].take().unwrap();
                let pong_tx = pong_txs[i].take().unwrap();

                if spin(ping_rx).is_err() {
                    return Ok(());
                }

                pong_tx.send(()).unwrap();
            }

            Ok::<(), ()>(())
        });
        spin(fut).unwrap();
    });

    let fut = future::lazy(|_| {
        let mut i = 0;

        b.iter(|| {
            let ping_tx = ping_txs[i].take().unwrap();
            let pong_rx = pong_rxs[i].take().unwrap();

            ping_tx.send(()).unwrap();
            spin(pong_rx).unwrap();

            i += 1;
        });

        Ok::<(), ()>(())
    });
    spin(fut).unwrap();
}
