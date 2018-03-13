extern crate tokio_threadpool;
extern crate tokio_executor;
extern crate futures;
extern crate env_logger;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

use tokio_threadpool::*;

#[cfg(not(feature = "unstable-futures"))]
use futures::{Poll, Sink, Stream, Async, Future};
#[cfg(not(feature = "unstable-futures"))]
use futures::future::lazy;

#[cfg(feature = "unstable-futures")]
use futures2::prelude::*;
#[cfg(feature = "unstable-futures")]
use futures2::future::lazy;

use std::cell::Cell;
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

thread_local!(static FOO: Cell<u32> = Cell::new(0));

#[cfg(not(feature = "unstable-futures"))]
fn spawn_pool<F>(pool: &mut Sender, f: F)
    where F: Future<Item = (), Error = ()> + Send + 'static
{
    pool.spawn(f).unwrap()
}
#[cfg(feature = "unstable-futures")]
fn spawn_pool<F>(pool: &mut Sender, f: F)
    where F: Future<Item = (), Error = ()> + Send + 'static
{
    futures2::executor::Executor::spawn(
        pool,
        Box::new(f.map_err(|_| panic!()))
    ).unwrap()
}

#[cfg(not(feature = "unstable-futures"))]
fn spawn_default<F>(f: F)
    where F: Future<Item = (), Error = ()> + Send + 'static
{
    tokio_executor::spawn(f)
}
#[cfg(feature = "unstable-futures")]
fn spawn_default<F>(f: F)
    where F: Future<Item = (), Error = ()> + Send + 'static
{
    tokio_executor::spawn2(Box::new(f.map_err(|_| panic!())))
}

fn ignore_results<F: Future + Send + 'static>(f: F) -> Box<Future<Item = (), Error = ()> + Send> {
    Box::new(f.map(|_| ()).map_err(|_| ()))
}

#[cfg(feature = "unstable-futures")]
fn await_shutdown(shutdown: Shutdown) {
    futures::Future::wait(shutdown).unwrap()
}
#[cfg(not(feature = "unstable-futures"))]
fn await_shutdown(shutdown: Shutdown) {
    shutdown.wait().unwrap()
}

#[cfg(not(feature = "unstable-futures"))]
fn block_on<F: Future>(f: F) -> Result<F::Item, F::Error> {
    f.wait()
}
#[cfg(feature = "unstable-futures")]
fn block_on<F: Future>(f: F) -> Result<F::Item, F::Error> {
    futures2::executor::block_on(f)
}

#[test]
fn natural_shutdown_simple_futures() {
    let _ = ::env_logger::init();

    for _ in 0..1_000 {
        static NUM_INC: AtomicUsize = ATOMIC_USIZE_INIT;
        static NUM_DEC: AtomicUsize = ATOMIC_USIZE_INIT;

        FOO.with(|f| {
            f.set(1);

            let pool = Builder::new()
                .around_worker(|w, _| {
                    NUM_INC.fetch_add(1, Relaxed);
                    w.run();
                    NUM_DEC.fetch_add(1, Relaxed);
                })
                .build();
            let mut tx = pool.sender().clone();

            let a = {
                let (t, rx) = mpsc::channel();
                spawn_pool(&mut tx, lazy(move || {
                    // Makes sure this runs on a worker thread
                    FOO.with(|f| assert_eq!(f.get(), 0));

                    t.send("one").unwrap();
                    Ok(())
                }));
                rx
            };

            let b = {
                let (t, rx) = mpsc::channel();
                spawn_pool(&mut tx, lazy(move || {
                    // Makes sure this runs on a worker thread
                    FOO.with(|f| assert_eq!(f.get(), 0));

                    t.send("two").unwrap();
                    Ok(())
                }));
                rx
            };

            drop(tx);

            assert_eq!("one", a.recv().unwrap());
            assert_eq!("two", b.recv().unwrap());

            // Wait for the pool to shutdown
            await_shutdown(pool.shutdown());

            // Assert that at least one thread started
            let num_inc = NUM_INC.load(Relaxed);
            assert!(num_inc > 0);

            // Assert that all threads shutdown
            let num_dec = NUM_DEC.load(Relaxed);
            assert_eq!(num_inc, num_dec);
        });
    }
}

#[test]
fn force_shutdown_drops_futures() {
    let _ = ::env_logger::init();

    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        #[cfg(not(feature = "unstable-futures"))]
        impl Future for Never {
            type Item = ();
            type Error = ();

            fn poll(&mut self) -> Poll<(), ()> {
                Ok(Async::NotReady)
            }
        }

        #[cfg(feature = "unstable-futures")]
        impl Future for Never {
            type Item = ();
            type Error = ();

            fn poll(&mut self, _: &mut futures2::task::Context) -> Poll<(), ()> {
                Ok(Async::Pending)
            }
        }

        impl Drop for Never {
            fn drop(&mut self) {
                self.0.fetch_add(1, Relaxed);
            }
        }

        let a = num_inc.clone();
        let b = num_dec.clone();

        let mut pool = Builder::new()
            .around_worker(move |w, _| {
                a.fetch_add(1, Relaxed);
                w.run();
                b.fetch_add(1, Relaxed);
            })
            .build();
        let mut tx = pool.sender().clone();

        spawn_pool(&mut tx, Never(num_drop.clone()));

        // Wait for the pool to shutdown
        await_shutdown(pool.shutdown_now());

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
    let _ = ::env_logger::init();

    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        #[cfg(not(feature = "unstable-futures"))]
        impl Future for Never {
            type Item = ();
            type Error = ();

            fn poll(&mut self) -> Poll<(), ()> {
                Ok(Async::NotReady)
            }
        }

        #[cfg(feature = "unstable-futures")]
        impl Future for Never {
            type Item = ();
            type Error = ();

            fn poll(&mut self, _: &mut futures2::task::Context) -> Poll<(), ()> {
                Ok(Async::Pending)
            }
        }

        impl Drop for Never {
            fn drop(&mut self) {
                self.0.fetch_add(1, Relaxed);
            }
        }

        let a = num_inc.clone();
        let b = num_dec.clone();

        let mut pool = Builder::new()
            .around_worker(move |w, _| {
                a.fetch_add(1, Relaxed);
                w.run();
                b.fetch_add(1, Relaxed);
            })
            .build();
        let mut tx = pool.sender().clone();

        spawn_pool(&mut tx, Never(num_drop.clone()));

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
fn thread_shutdown_timeout() {
    use std::sync::Mutex;

    let _ = ::env_logger::init();

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let (complete_tx, complete_rx) = mpsc::channel();

    let t = Mutex::new(shutdown_tx);

    let pool = Builder::new()
        .keep_alive(Some(Duration::from_millis(200)))
        .around_worker(move |w, _| {
            w.run();
            // There could be multiple threads here
            let _ = t.lock().unwrap().send(());
        })
        .build();
    let mut tx = pool.sender().clone();

    let t = complete_tx.clone();
    spawn_pool(&mut tx, lazy(move || {
        t.send(()).unwrap();
        Ok(())
    }));

    // The future completes
    complete_rx.recv().unwrap();

    // The thread shuts down eventually
    shutdown_rx.recv().unwrap();

    // Futures can still be run
    spawn_pool(&mut tx, lazy(move || {
        complete_tx.send(()).unwrap();
        Ok(())
    }));

    complete_rx.recv().unwrap();

    await_shutdown(pool.shutdown());
}

#[test]
fn many_oneshot_futures() {
    const NUM: usize = 10_000;

    let _ = ::env_logger::init();

    for _ in 0..50 {
        let pool = ThreadPool::new();
        let mut tx = pool.sender().clone();
        let cnt = Arc::new(AtomicUsize::new(0));

        for _ in 0..NUM {
            let cnt = cnt.clone();
            spawn_pool(&mut tx, lazy(move || {
                cnt.fetch_add(1, Relaxed);
                Ok(())
            }));
        }

        // Wait for the pool to shutdown
        await_shutdown(pool.shutdown());

        let num = cnt.load(Relaxed);
        assert_eq!(num, NUM);
    }
}

#[test]
fn many_multishot_futures() {
    #[cfg(not(feature = "unstable-futures"))]
    use futures::sync::mpsc;

    #[cfg(feature = "unstable-futures")]
    use futures2::channel::mpsc;

    const CHAIN: usize = 200;
    const CYCLES: usize = 5;
    const TRACKS: usize = 50;

    let _ = ::env_logger::init();

    for _ in 0..50 {
        let pool = ThreadPool::new();
        let mut pool_tx = pool.sender().clone();

        let mut start_txs = Vec::with_capacity(TRACKS);
        let mut final_rxs = Vec::with_capacity(TRACKS);

        for _ in 0..TRACKS {
            let (start_tx, mut chain_rx) = mpsc::channel(10);

            for _ in 0..CHAIN {
                let (next_tx, next_rx) = mpsc::channel(10);

                let rx = chain_rx
                    .map_err(|e| panic!("{:?}", e));

                // Forward all the messages
                spawn_pool(&mut pool_tx, next_tx
                    .send_all(rx)
                    .map(|_| ())
                    .map_err(|e| panic!("{:?}", e))
                );

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
            spawn_pool(&mut pool_tx, ignore_results(task));

            start_txs.push(start_tx);
            final_rxs.push(final_rx);
        }

        for start_tx in start_txs {
            block_on(start_tx.send("ping")).unwrap();
        }

        for final_rx in final_rxs {
            block_on(final_rx.into_future()).unwrap();
        }

        // Shutdown the pool
        await_shutdown(pool.shutdown());
    }
}

#[test]
fn global_executor_is_configured() {
    let pool = ThreadPool::new();
    let mut tx = pool.sender().clone();

    let (signal_tx, signal_rx) = mpsc::channel();

    spawn_pool(&mut tx, lazy(move || {
        spawn_default(lazy(move || {
            signal_tx.send(()).unwrap();
            Ok(())
        }));

        Ok(())
    }));

    signal_rx.recv().unwrap();

    await_shutdown(pool.shutdown());
}

#[test]
fn new_threadpool_is_idle() {
    let pool = ThreadPool::new();
    await_shutdown(pool.shutdown_on_idle());
}

#[test]
fn busy_threadpool_is_not_idle() {
    #[cfg(not(feature = "unstable-futures"))]
    use futures::sync::oneshot;

    #[cfg(feature = "unstable-futures")]
    use futures2::channel::oneshot;

    let pool = ThreadPool::new();
    let mut tx = pool.sender().clone();

    let (term_tx, term_rx) = oneshot::channel();

    spawn_pool(&mut tx, term_rx.then(|_| {
        Ok(())
    }));

    let mut idle = pool.shutdown_on_idle();

    struct IdleFut<'a>(&'a mut Shutdown);

    #[cfg(not(feature = "unstable-futures"))]
    impl<'a> Future for IdleFut<'a> {
        type Item = ();
        type Error = ();
        fn poll(&mut self) -> Poll<(), ()> {
            assert!(self.0.poll().unwrap().is_not_ready());
            Ok(Async::Ready(()))
        }
    }

    #[cfg(feature = "unstable-futures")]
    impl<'a> Future for IdleFut<'a> {
        type Item = ();
        type Error = ();
        fn poll(&mut self, cx: &mut futures2::task::Context) -> Poll<(), ()> {
            assert!(self.0.poll(cx).unwrap().is_pending());
            Ok(Async::Ready(()))
        }
    }

    block_on(IdleFut(&mut idle)).unwrap();

    term_tx.send(()).unwrap();

    await_shutdown(idle);
}

#[test]
fn panic_in_task() {
    let pool = ThreadPool::new();
    let mut tx = pool.sender().clone();

    struct Boom;

    #[cfg(not(feature = "unstable-futures"))]
    impl Future for Boom {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            panic!();
        }
    }

    #[cfg(feature = "unstable-futures")]
    impl Future for Boom {
        type Item = ();
        type Error = ();

        fn poll(&mut self, _cx: &mut futures2::task::Context) -> Poll<(), ()> {
            panic!();
        }
    }

    impl Drop for Boom {
        fn drop(&mut self) {
            assert!(::std::thread::panicking());
        }
    }

    spawn_pool(&mut tx, Boom);

    await_shutdown(pool.shutdown_on_idle());
}
