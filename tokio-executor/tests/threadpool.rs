#![warn(rust_2018_idioms)]

use tokio_executor::park::{Park, Unpark};
use tokio_executor::threadpool;
use tokio_executor::threadpool::park::{DefaultPark, DefaultUnpark};
use tokio_executor::threadpool::*;
use tokio_test::assert_pending;

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::sync::{mpsc, Arc};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

thread_local!(static FOO: Cell<u32> = Cell::new(0));

#[test]
fn natural_shutdown_simple_futures() {
    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));

        FOO.with(|f| {
            f.set(1);

            let pool = {
                let num_inc = num_inc.clone();
                let num_dec = num_dec.clone();

                Builder::new()
                    .around_worker(move |w| {
                        num_inc.fetch_add(1, Relaxed);
                        w.run();
                        num_dec.fetch_add(1, Relaxed);
                    })
                    .build()
            };

            let tx = pool.sender().clone();

            let a = {
                let (t, rx) = mpsc::channel();
                tx.spawn(async move {
                    // Makes sure this runs on a worker thread
                    FOO.with(|f| assert_eq!(f.get(), 0));

                    t.send("one").unwrap();
                })
                .unwrap();
                rx
            };

            let b = {
                let (t, rx) = mpsc::channel();
                tx.spawn(async move {
                    // Makes sure this runs on a worker thread
                    FOO.with(|f| assert_eq!(f.get(), 0));

                    t.send("two").unwrap();
                })
                .unwrap();
                rx
            };

            drop(tx);

            assert_eq!("one", a.recv().unwrap());
            assert_eq!("two", b.recv().unwrap());

            // Wait for the pool to shutdown
            pool.shutdown().wait();

            // Assert that at least one thread started
            let num_inc = num_inc.load(Relaxed);
            assert!(num_inc > 0);

            // Assert that all threads shutdown
            let num_dec = num_dec.load(Relaxed);
            assert_eq!(num_inc, num_dec);
        });
    }
}

#[test]
fn force_shutdown_drops_futures() {
    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        impl Future for Never {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
                Poll::Pending
            }
        }

        impl Drop for Never {
            fn drop(&mut self) {
                self.0.fetch_add(1, Relaxed);
            }
        }

        let a = num_inc.clone();
        let b = num_dec.clone();

        let pool = Builder::new()
            .around_worker(move |w| {
                a.fetch_add(1, Relaxed);
                w.run();
                b.fetch_add(1, Relaxed);
            })
            .build();
        let tx = pool.sender().clone();

        tx.spawn(Never(num_drop.clone())).unwrap();

        // Wait for the pool to shutdown
        pool.shutdown_now().wait();

        // Assert that only a single thread was spawned.
        let a = num_inc.load(Relaxed);
        assert!(a >= 1);

        // Assert that all threads shutdown
        let b = num_dec.load(Relaxed);
        assert_eq!(a, b);

        // Assert that the future was dropped
        let c = num_drop.load(Relaxed);
        assert_eq!(c, 1);
    }
}

#[test]
fn drop_threadpool_drops_futures() {
    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        impl Future for Never {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
                Poll::Pending
            }
        }

        impl Drop for Never {
            fn drop(&mut self) {
                self.0.fetch_add(1, Relaxed);
            }
        }

        let a = num_inc.clone();
        let b = num_dec.clone();

        let pool = Builder::new()
            .max_blocking(2)
            .pool_size(20)
            .around_worker(move |w| {
                a.fetch_add(1, Relaxed);
                w.run();
                b.fetch_add(1, Relaxed);
            })
            .build();
        let tx = pool.sender().clone();

        tx.spawn(Never(num_drop.clone())).unwrap();

        // Wait for the pool to shutdown
        drop(pool);

        // Assert that only a single thread was spawned.
        let a = num_inc.load(Relaxed);
        assert!(a >= 1);

        // Assert that all threads shutdown
        let b = num_dec.load(Relaxed);
        assert_eq!(a, b);

        // Assert that the future was dropped
        let c = num_drop.load(Relaxed);
        assert_eq!(c, 1);
    }
}

#[test]
fn many_oneshot_futures() {
    const NUM: usize = 10_000;

    for _ in 0..50 {
        let pool = ThreadPool::new();
        let tx = pool.sender().clone();
        let cnt = Arc::new(AtomicUsize::new(0));

        for _ in 0..NUM {
            let cnt = cnt.clone();
            tx.spawn(async move {
                cnt.fetch_add(1, Relaxed);
            })
            .unwrap();
        }

        // Wait for the pool to shutdown
        pool.shutdown().wait();

        let num = cnt.load(Relaxed);
        assert_eq!(num, NUM);
    }
}

#[test]
fn many_multishot_futures() {
    use tokio::sync::mpsc;

    const CHAIN: usize = 200;
    const CYCLES: usize = 5;
    const TRACKS: usize = 50;

    for _ in 0..50 {
        let pool = ThreadPool::new();
        let pool_tx = pool.sender().clone();

        let mut start_txs = Vec::with_capacity(TRACKS);
        let mut final_rxs = Vec::with_capacity(TRACKS);

        for _ in 0..TRACKS {
            let (start_tx, mut chain_rx) = mpsc::channel(10);

            for _ in 0..CHAIN {
                let (mut next_tx, next_rx) = mpsc::channel(10);

                // Forward all the messages
                pool_tx
                    .spawn(async move {
                        while let Some(v) = chain_rx.recv().await {
                            next_tx.send(v).await.unwrap();
                        }
                    })
                    .unwrap();

                chain_rx = next_rx;
            }

            // This final task cycles if needed
            let (mut final_tx, final_rx) = mpsc::channel(10);
            let mut cycle_tx = start_tx.clone();
            let mut rem = CYCLES;

            pool_tx
                .spawn(async move {
                    for _ in 0..CYCLES {
                        let msg = chain_rx.recv().await.unwrap();

                        rem -= 1;

                        if rem == 0 {
                            final_tx.send(msg).await.unwrap();
                        } else {
                            cycle_tx.send(msg).await.unwrap();
                        }
                    }
                })
                .unwrap();

            start_txs.push(start_tx);
            final_rxs.push(final_rx);
        }

        {
            let mut e = tokio_executor::enter().unwrap();

            e.block_on(async move {
                for mut start_tx in start_txs {
                    start_tx.send("ping").await.unwrap();
                }

                for mut final_rx in final_rxs {
                    final_rx.recv().await.unwrap();
                }
            });
        }

        // Shutdown the pool
        pool.shutdown().wait();
    }
}

#[test]
fn global_executor_is_configured() {
    let pool = ThreadPool::new();
    let tx = pool.sender().clone();

    let (signal_tx, signal_rx) = mpsc::channel();

    tx.spawn(async move {
        tokio_executor::spawn(async move {
            signal_tx.send(()).unwrap();
        });
    })
    .unwrap();

    signal_rx.recv().unwrap();

    pool.shutdown().wait();
}

#[test]
fn new_threadpool_is_idle() {
    let pool = ThreadPool::new();
    pool.shutdown_on_idle().wait();
}

#[test]
fn busy_threadpool_is_not_idle() {
    use tokio_sync::oneshot;

    // let pool = ThreadPool::new();
    let pool = Builder::new().pool_size(4).max_blocking(2).build();
    let tx = pool.sender().clone();

    let (term_tx, term_rx) = oneshot::channel();

    tx.spawn(async move {
        term_rx.await.unwrap();
    })
    .unwrap();

    let mut idle = pool.shutdown_on_idle();

    struct IdleFut<'a>(&'a mut Shutdown);

    impl Future for IdleFut<'_> {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            assert_pending!(Pin::new(&mut self.as_mut().0).poll(cx));
            Poll::Ready(())
        }
    }

    let idle_fut = IdleFut(&mut idle);
    tokio_executor::enter().unwrap().block_on(idle_fut);

    term_tx.send(()).unwrap();

    let idle_fut = IdleFut(&mut idle);
    tokio_executor::enter().unwrap().block_on(idle_fut);
}

#[test]
fn panic_in_task() {
    let pool = ThreadPool::new();
    let tx = pool.sender().clone();

    struct Boom;

    impl Future for Boom {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            panic!();
        }
    }

    impl Drop for Boom {
        fn drop(&mut self) {
            assert!(::std::thread::panicking());
        }
    }

    tx.spawn(Boom).unwrap();

    pool.shutdown_on_idle().wait();
}

#[test]
fn count_panics() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_ = counter.clone();
    let pool = threadpool::Builder::new()
        .panic_handler(move |_err| {
            // We caught a panic.
            counter_.fetch_add(1, Relaxed);
        })
        .build();
    // Spawn a future that will panic.
    pool.spawn(async { panic!() });
    pool.shutdown_on_idle().wait();
    let counter = counter.load(Relaxed);
    assert_eq!(counter, 1);
}

#[test]
fn multi_threadpool() {
    use tokio_sync::oneshot;

    let pool1 = ThreadPool::new();
    let pool2 = ThreadPool::new();

    let (tx, rx) = oneshot::channel();
    let (done_tx, done_rx) = mpsc::channel();

    pool2.spawn(async move {
        rx.await.unwrap();
        done_tx.send(()).unwrap();
    });

    pool1.spawn(async move {
        tx.send(()).unwrap();
    });

    done_rx.recv().unwrap();
}

#[test]
fn eagerly_drops_futures() {
    use std::sync::mpsc;

    struct MyPark {
        inner: DefaultPark,
        #[allow(dead_code)]
        park_tx: mpsc::SyncSender<()>,
        unpark_tx: mpsc::SyncSender<()>,
    }

    impl Park for MyPark {
        type Unpark = MyUnpark;
        type Error = <DefaultPark as Park>::Error;

        fn unpark(&self) -> Self::Unpark {
            MyUnpark {
                inner: self.inner.unpark(),
                unpark_tx: self.unpark_tx.clone(),
            }
        }

        fn park(&mut self) -> Result<(), Self::Error> {
            self.inner.park()
        }

        fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
            self.inner.park_timeout(duration)
        }
    }

    struct MyUnpark {
        inner: DefaultUnpark,
        #[allow(dead_code)]
        unpark_tx: mpsc::SyncSender<()>,
    }

    impl Unpark for MyUnpark {
        fn unpark(&self) {
            self.inner.unpark()
        }
    }

    let (task_tx, task_rx) = mpsc::channel();
    let (drop_tx, drop_rx) = mpsc::channel();
    let (park_tx, park_rx) = mpsc::sync_channel(0);
    let (unpark_tx, unpark_rx) = mpsc::sync_channel(0);

    let pool = threadpool::Builder::new()
        .custom_park(move |_| MyPark {
            inner: DefaultPark::new(),
            park_tx: park_tx.clone(),
            unpark_tx: unpark_tx.clone(),
        })
        .build();

    struct MyTask {
        task_tx: Option<mpsc::Sender<Waker>>,
        drop_tx: mpsc::Sender<()>,
    }

    impl Future for MyTask {
        type Output = ();

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if let Some(tx) = self.get_mut().task_tx.take() {
                tx.send(cx.waker().clone()).unwrap();
            }

            Poll::Pending
        }
    }

    impl Drop for MyTask {
        fn drop(&mut self) {
            self.drop_tx.send(()).unwrap();
        }
    }

    pool.spawn(MyTask {
        task_tx: Some(task_tx),
        drop_tx,
    });

    // Wait until we get the task handle.
    let task = task_rx.recv().unwrap();

    // Drop the pool, this should result in futures being forcefully dropped.
    drop(pool);

    // Make sure `MyPark` and `MyUnpark` were dropped during shutdown.
    assert_eq!(park_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    assert_eq!(unpark_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected));

    // If the future is forcefully dropped, then we will get a signal here.
    drop_rx.recv().unwrap();

    // Ensure `task` lives until after the test completes.
    drop(task);
}
