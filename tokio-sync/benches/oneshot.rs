#![feature(test)]

extern crate futures;
extern crate test;
extern crate tokio_sync;

mod tokio {
    use futures::{future, Async, Future};
    use test::Bencher;
    use tokio_sync::oneshot;

    #[bench]
    fn new(b: &mut Bencher) {
        b.iter(|| {
            let _ = ::test::black_box(&oneshot::channel::<i32>());
        })
    }

    #[bench]
    fn same_thread_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            let _ = tx.send(1);

            assert_eq!(Async::Ready(1), rx.poll().unwrap());
        });
    }

    #[bench]
    fn same_thread_recv_multi_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            future::lazy(|| {
                let _ = rx.poll();
                let _ = rx.poll();
                let _ = rx.poll();
                let _ = rx.poll();

                let _ = tx.send(1);
                assert_eq!(Async::Ready(1), rx.poll().unwrap());

                Ok::<_, ()>(())
            })
            .wait()
            .unwrap();
        });
    }

    #[bench]
    fn multi_thread_send_recv(b: &mut Bencher) {
        const MAX: usize = 10_000_000;

        use std::thread;

        fn spin<F: Future>(mut f: F) -> Result<F::Item, F::Error> {
            use futures::Async::Ready;
            loop {
                match f.poll() {
                    Ok(Ready(v)) => return Ok(v),
                    Ok(_) => {}
                    Err(e) => return Err(e),
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
            future::lazy(|| {
                for i in 0..MAX {
                    let ping_rx = ping_rxs[i].take().unwrap();
                    let pong_tx = pong_txs[i].take().unwrap();

                    if spin(ping_rx).is_err() {
                        return Ok(());
                    }

                    pong_tx.send(()).unwrap();
                }

                Ok::<(), ()>(())
            })
            .wait()
            .unwrap();
        });

        future::lazy(|| {
            let mut i = 0;

            b.iter(|| {
                let ping_tx = ping_txs[i].take().unwrap();
                let pong_rx = pong_rxs[i].take().unwrap();

                ping_tx.send(()).unwrap();
                spin(pong_rx).unwrap();

                i += 1;
            });

            Ok::<(), ()>(())
        })
        .wait()
        .unwrap();
    }
}

mod legacy {
    use futures::sync::oneshot;
    use futures::{future, Async, Future};
    use test::Bencher;

    #[bench]
    fn new(b: &mut Bencher) {
        b.iter(|| {
            let _ = ::test::black_box(&oneshot::channel::<i32>());
        })
    }

    #[bench]
    fn same_thread_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            let _ = tx.send(1);

            assert_eq!(Async::Ready(1), rx.poll().unwrap());
        });
    }

    #[bench]
    fn same_thread_recv_multi_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            future::lazy(|| {
                let _ = rx.poll();
                let _ = rx.poll();
                let _ = rx.poll();
                let _ = rx.poll();

                let _ = tx.send(1);
                assert_eq!(Async::Ready(1), rx.poll().unwrap());

                Ok::<_, ()>(())
            })
            .wait()
            .unwrap();
        });
    }

    #[bench]
    fn multi_thread_send_recv(b: &mut Bencher) {
        const MAX: usize = 10_000_000;

        use std::thread;

        fn spin<F: Future>(mut f: F) -> Result<F::Item, F::Error> {
            use futures::Async::Ready;
            loop {
                match f.poll() {
                    Ok(Ready(v)) => return Ok(v),
                    Ok(_) => {}
                    Err(e) => return Err(e),
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
            future::lazy(|| {
                for i in 0..MAX {
                    let ping_rx = ping_rxs[i].take().unwrap();
                    let pong_tx = pong_txs[i].take().unwrap();

                    if spin(ping_rx).is_err() {
                        return Ok(());
                    }

                    pong_tx.send(()).unwrap();
                }

                Ok::<(), ()>(())
            })
            .wait()
            .unwrap();
        });

        future::lazy(|| {
            let mut i = 0;

            b.iter(|| {
                let ping_tx = ping_txs[i].take().unwrap();
                let pong_rx = pong_rxs[i].take().unwrap();

                ping_tx.send(()).unwrap();
                spin(pong_rx).unwrap();

                i += 1;
            });

            Ok::<(), ()>(())
        })
        .wait()
        .unwrap();
    }
}
