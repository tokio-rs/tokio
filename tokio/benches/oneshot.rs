#![warn(rust_2018_idioms)]

use std::{marker::Unpin, pin::Pin};

use tokio::sync::oneshot;

use criterion_bencher_compat as test;

use futures::{executor::block_on, future, task, Future, Poll};
use test::Bencher;

fn new(b: &mut Bencher<'_, '_>) {
    b.iter(|| {
        let _ = test::black_box(&oneshot::channel::<i32>());
    })
}

fn same_thread_send_recv(b: &mut Bencher<'_, '_>) {
    block_on(future::lazy(|cx| {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            let _ = tx.send(1);

            assert_eq!(
                Poll::Ready(Ok(1)),
                Pin::new(&mut rx).poll(cx).map_err(|_| ())
            );
        });
    }));
}

fn same_thread_recv_multi_send_recv(b: &mut Bencher<'_, '_>) {
    b.iter(|| {
        let (tx, mut rx) = oneshot::channel();
        let mut rx = Pin::new(&mut rx);

        block_on(future::lazy(|cx| {
            let _ = rx.as_mut().poll(cx);
            let _ = rx.as_mut().poll(cx);
            let _ = rx.as_mut().poll(cx);
            let _ = rx.as_mut().poll(cx);

            let _ = tx.send(1);
            assert_eq!(Poll::Ready(Ok(1)), rx.poll(cx).map_err(|_| ()));
        }));
    });
}

fn multi_thread_send_recv(b: &mut Bencher<'_, '_>) {
    const MAX: usize = 10_000_000;

    use std::thread;

    fn spin<F: Future + Unpin>(cx: &mut task::Context<'_>, mut f: F) -> F::Output {
        loop {
            match Pin::new(&mut f).poll(cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => {}
            }
        }
    }

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
        block_on(future::lazy(|cx| {
            for i in 0..MAX {
                let ping_rx = ping_rxs[i].take().unwrap();
                let pong_tx = pong_txs[i].take().unwrap();

                if spin(cx, ping_rx).is_err() {
                    return;
                }

                pong_tx.send(()).unwrap();
            }
        }));
    });

    block_on(future::lazy(|cx| {
        let mut i = 0;

        b.iter(|| {
            let ping_tx = ping_txs[i].take().unwrap();
            let pong_rx = pong_rxs[i].take().unwrap();

            ping_tx.send(()).unwrap();
            spin(cx, pong_rx).unwrap();

            i += 1;
        });
    }))
}

criterion_bencher_compat::benchmark_group!(
    oneshot,
    new,
    same_thread_recv_multi_send_recv,
    same_thread_send_recv,
    multi_thread_send_recv
);
criterion_bencher_compat::benchmark_main!(oneshot);
