extern crate env_logger;
extern crate futures;
extern crate tokio_executor;
extern crate tokio_threadpool;

use tokio_executor::park::{Park, Unpark};
use tokio_threadpool::park::{DefaultPark, DefaultUnpark};
use tokio_threadpool::*;

use futures::future::lazy;
use futures::{Async, Future, Poll, Sink, Stream};

use std::cell::Cell;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::sync::{mpsc, Arc};
use std::time::Duration;

thread_local!(static FOO: Cell<u32> = Cell::new(0));

fn ignore_results<F: Future + Send + 'static>(
    f: F,
) -> Box<dyn Future<Item = (), Error = ()> + Send> {
    Box::new(f.map(|_| ()).map_err(|_| ()))
}

#[test]
fn natural_shutdown_simple_futures() {
    let _ = ::env_logger::try_init();

    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));

        FOO.with(|f| {
            f.set(1);

            let pool = {
                let num_inc = num_inc.clone();
                let num_dec = num_dec.clone();

                Builder::new()
                    .around_worker(move |w, _| {
                        num_inc.fetch_add(1, Relaxed);
                        w.run();
                        num_dec.fetch_add(1, Relaxed);
                    })
                    .build()
            };

            let tx = pool.sender().clone();

            let a = {
                let (t, rx) = mpsc::channel();
                tx.spawn(lazy(move || {
                    // Makes sure this runs on a worker thread
                    FOO.with(|f| assert_eq!(f.get(), 0));

                    t.send("one").unwrap();
                    Ok(())
                }))
                .unwrap();
                rx
            };

            let b = {
                let (t, rx) = mpsc::channel();
                tx.spawn(lazy(move || {
                    // Makes sure this runs on a worker thread
                    FOO.with(|f| assert_eq!(f.get(), 0));

                    t.send("two").unwrap();
                    Ok(())
                }))
                .unwrap();
                rx
            };

            drop(tx);

            assert_eq!("one", a.recv().unwrap());
            assert_eq!("two", b.recv().unwrap());

            // Wait for the pool to shutdown
            pool.shutdown().wait().unwrap();

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
    let _ = ::env_logger::try_init();

    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        impl Future for Never {
            type Item = ();
            type Error = ();

            fn poll(&mut self) -> Poll<(), ()> {
                Ok(Async::NotReady)
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
            .around_worker(move |w, _| {
                a.fetch_add(1, Relaxed);
                w.run();
                b.fetch_add(1, Relaxed);
            })
            .build();
        let tx = pool.sender().clone();

        tx.spawn(Never(num_drop.clone())).unwrap();

        // Wait for the pool to shutdown
        pool.shutdown_now().wait().unwrap();

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
    let _ = ::env_logger::try_init();

    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        impl Future for Never {
            type Item = ();
            type Error = ();

            fn poll(&mut self) -> Poll<(), ()> {
                Ok(Async::NotReady)
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
            .around_worker(move |w, _| {
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

    let _ = ::env_logger::try_init();

    for _ in 0..50 {
        let pool = ThreadPool::new();
        let tx = pool.sender().clone();
        let cnt = Arc::new(AtomicUsize::new(0));

        for _ in 0..NUM {
            let cnt = cnt.clone();
            tx.spawn(lazy(move || {
                cnt.fetch_add(1, Relaxed);
                Ok(())
            }))
            .unwrap();
        }

        // Wait for the pool to shutdown
        pool.shutdown().wait().unwrap();

        let num = cnt.load(Relaxed);
        assert_eq!(num, NUM);
    }
}

#[test]
fn many_multishot_futures() {
    use futures::sync::mpsc;

    const CHAIN: usize = 200;
    const CYCLES: usize = 5;
    const TRACKS: usize = 50;

    let _ = ::env_logger::try_init();

    for _ in 0..50 {
        let pool = ThreadPool::new();
        let pool_tx = pool.sender().clone();

        let mut start_txs = Vec::with_capacity(TRACKS);
        let mut final_rxs = Vec::with_capacity(TRACKS);

        for _ in 0..TRACKS {
            let (start_tx, mut chain_rx) = mpsc::channel(10);

            for _ in 0..CHAIN {
                let (next_tx, next_rx) = mpsc::channel(10);

                let rx = chain_rx.map_err(|e| panic!("{:?}", e));

                // Forward all the messages
                pool_tx
                    .spawn(
                        next_tx
                            .send_all(rx)
                            .map(|_| ())
                            .map_err(|e| panic!("{:?}", e)),
                    )
                    .unwrap();

                chain_rx = next_rx;
            }

            // This final task cycles if needed
            let (final_tx, final_rx) = mpsc::channel(10);
            let cycle_tx = start_tx.clone();
            let mut rem = CYCLES;

            let task = chain_rx.take(CYCLES as u64).for_each(move |msg| {
                rem -= 1;
                let send = if rem == 0 {
                    final_tx.clone().send(msg)
                } else {
                    cycle_tx.clone().send(msg)
                };

                send.then(|res| {
                    res.unwrap();
                    Ok(())
                })
            });
            pool_tx.spawn(ignore_results(task)).unwrap();

            start_txs.push(start_tx);
            final_rxs.push(final_rx);
        }

        for start_tx in start_txs {
            start_tx.send("ping").wait().unwrap();
        }

        for final_rx in final_rxs {
            final_rx.wait().next().unwrap().unwrap();
        }

        // Shutdown the pool
        pool.shutdown().wait().unwrap();
    }
}

#[test]
fn global_executor_is_configured() {
    let pool = ThreadPool::new();
    let tx = pool.sender().clone();

    let (signal_tx, signal_rx) = mpsc::channel();

    tx.spawn(lazy(move || {
        tokio_executor::spawn(lazy(move || {
            signal_tx.send(()).unwrap();
            Ok(())
        }));

        Ok(())
    }))
    .unwrap();

    signal_rx.recv().unwrap();

    pool.shutdown().wait().unwrap();
}

#[test]
fn new_threadpool_is_idle() {
    let pool = ThreadPool::new();
    pool.shutdown_on_idle().wait().unwrap();
}

#[test]
fn busy_threadpool_is_not_idle() {
    use futures::sync::oneshot;

    // let pool = ThreadPool::new();
    let pool = Builder::new().pool_size(4).max_blocking(2).build();
    let tx = pool.sender().clone();

    let (term_tx, term_rx) = oneshot::channel();

    tx.spawn(term_rx.then(|_| Ok(()))).unwrap();

    let mut idle = pool.shutdown_on_idle();

    struct IdleFut<'a>(&'a mut Shutdown);

    impl<'a> Future for IdleFut<'a> {
        type Item = ();
        type Error = ();
        fn poll(&mut self) -> Poll<(), ()> {
            assert!(self.0.poll().unwrap().is_not_ready());
            Ok(Async::Ready(()))
        }
    }

    IdleFut(&mut idle).wait().unwrap();

    term_tx.send(()).unwrap();

    idle.wait().unwrap();
}

#[test]
fn panic_in_task() {
    let pool = ThreadPool::new();
    let tx = pool.sender().clone();

    struct Boom;

    impl Future for Boom {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            panic!();
        }
    }

    impl Drop for Boom {
        fn drop(&mut self) {
            assert!(::std::thread::panicking());
        }
    }

    tx.spawn(Boom).unwrap();

    pool.shutdown_on_idle().wait().unwrap();
}

#[test]
fn count_panics() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_ = counter.clone();
    let pool = tokio_threadpool::Builder::new()
        .panic_handler(move |_err| {
            // We caught a panic.
            counter_.fetch_add(1, Relaxed);
        })
        .build();
    // Spawn a future that will panic.
    pool.spawn(lazy(|| -> Result<(), ()> { panic!() }));
    pool.shutdown_on_idle().wait().unwrap();
    let counter = counter.load(Relaxed);
    assert_eq!(counter, 1);
}

#[test]
fn multi_threadpool() {
    use futures::sync::oneshot;

    let pool1 = ThreadPool::new();
    let pool2 = ThreadPool::new();

    let (tx, rx) = oneshot::channel();
    let (done_tx, done_rx) = mpsc::channel();

    pool2.spawn({
        rx.and_then(move |_| {
            done_tx.send(()).unwrap();
            Ok(())
        })
        .map_err(|e| panic!("err={:?}", e))
    });

    pool1.spawn(lazy(move || {
        tx.send(()).unwrap();
        Ok(())
    }));

    done_rx.recv().unwrap();
}

#[test]
fn eagerly_drops_futures() {
    use futures::future::{empty, lazy, Future};
    use futures::task;
    use std::sync::mpsc;

    struct NotifyOnDrop(mpsc::Sender<()>);

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
        }
    }

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

    // Get the signal that the handler dropped.
    let notify_on_drop = NotifyOnDrop(drop_tx);

    let pool = tokio_threadpool::Builder::new()
        .custom_park(move |_| MyPark {
            inner: DefaultPark::new(),
            park_tx: park_tx.clone(),
            unpark_tx: unpark_tx.clone(),
        })
        .build();

    pool.spawn(lazy(move || {
        // Get a handle to the current task.
        let task = task::current();

        // Send it to the main thread to hold on to.
        task_tx.send(task).unwrap();

        // This future will never resolve, it is only used to hold on to thee
        // `notify_on_drop` handle.
        empty::<(), ()>().then(move |_| {
            // This code path should never be reached.
            if true {
                panic!()
            }

            // Explicitly drop `notify_on_drop` here, this is mostly to ensure
            // that the `notify_on_drop` handle gets moved into the task. It
            // will actually get dropped when the runtime is dropped.
            drop(notify_on_drop);

            Ok(())
        })
    }));

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
