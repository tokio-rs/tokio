#![feature(test)]

extern crate tokio_sync;
extern crate futures;
extern crate test;

mod tokio {
    use tokio_sync::mpsc::*;
    use futures::{future, Async, Future, Stream, Sink};
    use test::{self, Bencher};
    use std::thread;

    #[bench]
    fn bounded_new(b: &mut Bencher) {
        b.iter(|| {
            let _ = test::black_box(&channel::<i32>(1_000));
        })
    }

    #[bench]
    fn unbounded_new(b: &mut Bencher) {
        b.iter(|| {
            let _ = test::black_box(&unbounded_channel::<i32>());
        })
    }

    #[bench]
    fn send_one_message(b: &mut Bencher) {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1_000);

            // Send
            tx.try_send(1).unwrap();

            // Receive
            assert_eq!(Async::Ready(Some(1)), rx.poll().unwrap());
        })
    }

    #[bench]
    fn bounded_rx_not_ready(b: &mut Bencher) {
        let (_tx, mut rx) = channel::<i32>(1_000);
        b.iter(|| {
            future::lazy(|| {
                assert!(rx.poll().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn bounded_tx_poll_ready(b: &mut Bencher) {
        let (mut tx, rx) = channel::<i32>(1);
        b.iter(|| {
            future::lazy(|| {
                assert!(tx.poll_ready().unwrap().is_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn bounded_tx_poll_not_ready(b: &mut Bencher) {
        let (mut tx, rx) = channel::<i32>(1);
        tx.try_send(1).unwrap();
        b.iter(|| {
            future::lazy(|| {
                assert!(tx.poll_ready().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn unbounded_rx_not_ready(b: &mut Bencher) {
        let (_tx, mut rx) = unbounded_channel::<i32>();
        b.iter(|| {
            future::lazy(|| {
                assert!(rx.poll().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn unbounded_rx_not_ready_x5(b: &mut Bencher) {
        let (_tx, mut rx) = unbounded_channel::<i32>();
        b.iter(|| {
            future::lazy(|| {
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn bounded_uncontended_1(b: &mut Bencher) {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1_000);

            for i in 0..1000 {
                tx.try_send(i).unwrap();
                // No need to create a task, because poll is not going to park.
                assert_eq!(Async::Ready(Some(i)), rx.poll().unwrap());
            }
        })
    }

    #[bench]
    fn bounded_uncontended_2(b: &mut Bencher) {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1000);

            for i in 0..1000 {
                tx.try_send(i).unwrap();
            }

            for i in 0..1000 {
                // No need to create a task, because poll is not going to park.
                assert_eq!(Async::Ready(Some(i)), rx.poll().unwrap());
            }
        })
    }

    #[bench]
    fn contended_unbounded_tx(b: &mut Bencher) {
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

            let rx = rx.wait()
                .take(4 * 1_000);

            for v in rx {
                test::black_box(v);
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

        for _ in 0..THREADS {
            let (tx, rx) = ::std::sync::mpsc::channel::<Sender<i32>>();
            txs.push(tx);

            threads.push(thread::spawn(move || {
                for tx in rx.iter() {
                    let mut tx = tx.wait();
                    for i in 0..ITERS {
                        tx.send(i as i32).unwrap();
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

            let rx = rx.wait()
                .take(THREADS * ITERS);

            for v in rx {
                test::black_box(v);
            }
        });

        drop(txs);

        for th in threads {
            th.join().unwrap();
        }
    }
}

mod legacy {
    use futures::{future, Async, Future, Stream, Sink};
    use futures::sync::mpsc::*;
    use test::{self, Bencher};
    use std::thread;

    #[bench]
    fn bounded_new(b: &mut Bencher) {
        b.iter(|| {
            let _ = test::black_box(&channel::<i32>(1_000));
        })
    }

    #[bench]
    fn unbounded_new(b: &mut Bencher) {
        b.iter(|| {
            let _ = test::black_box(&unbounded::<i32>());
        })
    }

    #[bench]
    fn send_one_message(b: &mut Bencher) {
        b.iter(|| {
            let (mut tx, mut rx) = channel(1_000);

            // Send
            tx.try_send(1).unwrap();

            // Receive
            assert_eq!(Ok(Async::Ready(Some(1))), rx.poll());
        })
    }

    #[bench]
    fn bounded_rx_not_ready(b: &mut Bencher) {
        let (_tx, mut rx) = channel::<i32>(1_000);
        b.iter(|| {
            future::lazy(|| {
                assert!(rx.poll().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn bounded_tx_poll_ready(b: &mut Bencher) {
        let (mut tx, rx) = channel::<i32>(0);
        b.iter(|| {
            future::lazy(|| {
                assert!(tx.poll_ready().unwrap().is_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn bounded_tx_poll_not_ready(b: &mut Bencher) {
        let (mut tx, rx) = channel::<i32>(0);
        tx.try_send(1).unwrap();
        b.iter(|| {
            future::lazy(|| {
                assert!(tx.poll_ready().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn unbounded_rx_not_ready(b: &mut Bencher) {
        let (_tx, mut rx) = unbounded::<i32>();
        b.iter(|| {
            future::lazy(|| {
                assert!(rx.poll().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn unbounded_rx_not_ready_x5(b: &mut Bencher) {
        let (_tx, mut rx) = unbounded::<i32>();
        b.iter(|| {
            future::lazy(|| {
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());
                assert!(rx.poll().unwrap().is_not_ready());

                Ok::<_, ()>(())
            }).wait().unwrap();
        })
    }

    #[bench]
    fn unbounded_uncontended_1(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = unbounded();

            for i in 0..1000 {
                UnboundedSender::unbounded_send(&tx, i).expect("send");
                // No need to create a task, because poll is not going to park.
                assert_eq!(Ok(Async::Ready(Some(i))), rx.poll());
            }
        })
    }

    #[bench]
    fn unbounded_uncontended_2(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = unbounded();

            for i in 0..1000 {
                UnboundedSender::unbounded_send(&tx, i).expect("send");
            }

            for i in 0..1000 {
                // No need to create a task, because poll is not going to park.
                assert_eq!(Ok(Async::Ready(Some(i))), rx.poll());
            }
        })
    }

    #[bench]
    fn multi_thread_unbounded_tx(b: &mut Bencher) {
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
            let (tx, rx) = channel::<i32>(1_000_000);

            for th in &txs {
                th.send(tx.clone()).unwrap();
            }

            drop(tx);

            let rx = rx.wait()
                .take(4 * 1_000);

            for v in rx {
                test::black_box(v);
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

        for _ in 0..THREADS {
            let (tx, rx) = ::std::sync::mpsc::channel::<Sender<i32>>();
            txs.push(tx);

            threads.push(thread::spawn(move || {
                for tx in rx.iter() {
                    let mut tx = tx.wait();
                    for i in 0..ITERS {
                        tx.send(i as i32).unwrap();
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

            let rx = rx.wait()
                .take(THREADS * ITERS);

            for v in rx {
                test::black_box(v);
            }
        });

        drop(txs);

        for th in threads {
            th.join().unwrap();
        }
    }
}
