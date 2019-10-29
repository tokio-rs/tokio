#![warn(rust_2018_idioms)]

use tokio::executor::park::{Park, Unpark};
use tokio::executor::thread_pool::*;

use futures_util::future::poll_fn;
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
fn shutdown_drops_futures() {
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

        let mut pool = Builder::new()
            .around_worker(move |_, work| {
                a.fetch_add(1, Relaxed);
                work();
                b.fetch_add(1, Relaxed);
            })
            .build();

        // let tx = pool.sender().clone();

        pool.spawn(Never(num_drop.clone()));

        // Wait for the pool to shutdown
        pool.shutdown_now();

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
    const NUM_THREADS: usize = 10;

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
            .num_threads(NUM_THREADS)
            .around_worker(move |_, work| {
                a.fetch_add(1, Relaxed);
                work();
                b.fetch_add(1, Relaxed);
            })
            .build();

        pool.spawn(Never(num_drop.clone()));

        // Wait for the pool to shutdown
        drop(pool);

        // Assert that all the threads spawned
        let a = num_inc.load(Relaxed);
        assert_eq!(a, NUM_THREADS);

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
    // used for notifying the main thread
    const NUM: usize = 10_000;

    for _ in 0..50 {
        let (tx, rx) = mpsc::channel();

        let mut pool = new_pool();
        let cnt = Arc::new(AtomicUsize::new(0));

        for _ in 0..NUM {
            let cnt = cnt.clone();
            let tx = tx.clone();

            pool.spawn(async move {
                let num = cnt.fetch_add(1, Relaxed) + 1;

                if num == NUM {
                    tx.send(()).unwrap();
                }
            });
        }

        rx.recv().unwrap();

        // Wait for the pool to shutdown
        pool.shutdown_now();
    }
}

#[test]
fn many_multishot_futures() {
    use tokio::sync::mpsc;

    const CHAIN: usize = 200;
    const CYCLES: usize = 5;
    const TRACKS: usize = 50;

    for _ in 0..50 {
        let pool = new_pool();
        let mut start_txs = Vec::with_capacity(TRACKS);
        let mut final_rxs = Vec::with_capacity(TRACKS);

        for _ in 0..TRACKS {
            let (start_tx, mut chain_rx) = mpsc::channel(10);

            for _ in 0..CHAIN {
                let (mut next_tx, next_rx) = mpsc::channel(10);

                // Forward all the messages
                pool.spawn(async move {
                    while let Some(v) = chain_rx.recv().await {
                        next_tx.send(v).await.unwrap();
                    }
                });

                chain_rx = next_rx;
            }

            // This final task cycles if needed
            let (mut final_tx, final_rx) = mpsc::channel(10);
            let mut cycle_tx = start_tx.clone();
            let mut rem = CYCLES;

            pool.spawn(async move {
                for _ in 0..CYCLES {
                    let msg = chain_rx.recv().await.unwrap();

                    rem -= 1;

                    if rem == 0 {
                        final_tx.send(msg).await.unwrap();
                    } else {
                        cycle_tx.send(msg).await.unwrap();
                    }
                }
            });

            start_txs.push(start_tx);
            final_rxs.push(final_rx);
        }

        {
            let mut e = tokio::executor::enter().unwrap();

            e.block_on(async move {
                for mut start_tx in start_txs {
                    start_tx.send("ping").await.unwrap();
                }

                for mut final_rx in final_rxs {
                    final_rx.recv().await.unwrap();
                }
            });
        }
    }
}

#[test]
fn global_executor_is_configured() {
    let pool = new_pool();

    let (signal_tx, signal_rx) = mpsc::channel();

    pool.spawn(async move {
        tokio::executor::spawn(async move {
            signal_tx.send(()).unwrap();
        });
    });

    signal_rx.recv().unwrap();
}

#[test]
fn new_threadpool_is_idle() {
    let mut pool = new_pool();
    pool.shutdown_now();
}

#[test]
fn panic_in_task() {
    let pool = new_pool();
    let (tx, rx) = mpsc::channel();

    struct Boom(mpsc::Sender<()>);

    impl Future for Boom {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            panic!();
        }
    }

    impl Drop for Boom {
        fn drop(&mut self) {
            assert!(::std::thread::panicking());
            self.0.send(()).unwrap();
        }
    }

    pool.spawn(Boom(tx));
    rx.recv().unwrap();
}

#[test]
fn multi_threadpool() {
    use tokio_sync::oneshot;

    let pool1 = new_pool();
    let pool2 = new_pool();

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
    use std::sync::{mpsc, Mutex};

    struct MyPark {
        rx: mpsc::Receiver<()>,
        tx: Mutex<mpsc::Sender<()>>,
        #[allow(dead_code)]
        park_tx: mpsc::SyncSender<()>,
        unpark_tx: mpsc::SyncSender<()>,
    }

    impl Park for MyPark {
        type Unpark = MyUnpark;
        type Error = ();

        fn unpark(&self) -> Self::Unpark {
            MyUnpark {
                tx: Mutex::new(self.tx.lock().unwrap().clone()),
                unpark_tx: self.unpark_tx.clone(),
            }
        }

        fn park(&mut self) -> Result<(), Self::Error> {
            let _ = self.rx.recv();
            Ok(())
        }

        fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
            let _ = self.rx.recv_timeout(duration);
            Ok(())
        }
    }

    struct MyUnpark {
        tx: Mutex<mpsc::Sender<()>>,
        #[allow(dead_code)]
        unpark_tx: mpsc::SyncSender<()>,
    }

    impl Unpark for MyUnpark {
        fn unpark(&self) {
            let _ = self.tx.lock().unwrap().send(());
        }
    }

    let (task_tx, task_rx) = mpsc::channel();
    let (drop_tx, drop_rx) = mpsc::channel();
    let (park_tx, park_rx) = mpsc::sync_channel(0);
    let (unpark_tx, unpark_rx) = mpsc::sync_channel(0);

    let pool = Builder::new().num_threads(4).build_with_park(move |_| {
        let (tx, rx) = mpsc::channel();
        MyPark {
            tx: Mutex::new(tx),
            rx,
            park_tx: park_tx.clone(),
            unpark_tx: unpark_tx.clone(),
        }
    });

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

#[test]
fn park_called_at_interval() {
    struct MyPark {
        park_light: Arc<AtomicBool>,
    }

    struct MyUnpark {}

    impl Park for MyPark {
        type Unpark = MyUnpark;
        type Error = ();

        fn unpark(&self) -> Self::Unpark {
            MyUnpark {}
        }

        fn park(&mut self) -> Result<(), Self::Error> {
            use std::thread;
            use std::time::Duration;

            thread::sleep(Duration::from_millis(1));
            Ok(())
        }

        fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
            if duration == Duration::from_millis(0) {
                self.park_light.store(true, Relaxed);
                Ok(())
            } else {
                self.park()
            }
        }
    }

    impl Unpark for MyUnpark {
        fn unpark(&self) {}
    }

    let park_light_1 = Arc::new(AtomicBool::new(false));
    let park_light_2 = park_light_1.clone();

    let (done_tx, done_rx) = mpsc::channel();

    // Use 1 thread to ensure the worker stays busy.
    let pool = Builder::new().num_threads(1).build_with_park(move |idx| {
        assert_eq!(idx, 0);
        MyPark {
            park_light: park_light_2.clone(),
        }
    });

    let mut cnt = 0;

    pool.spawn(poll_fn(move |cx| {
        let did_park_light = park_light_1.load(Relaxed);

        if did_park_light {
            // There is a bit of a race where the worker can tick a few times
            // before seeing the task
            assert!(cnt > 50);
            done_tx.send(()).unwrap();
            return Poll::Ready(());
        }

        cnt += 1;

        cx.waker().wake_by_ref();
        Poll::Pending
    }));

    done_rx.recv().unwrap();
}

fn new_pool() -> ThreadPool {
    Builder::new().num_threads(4).build()
}
