extern crate tokio_threadpool;

extern crate env_logger;
extern crate futures;
extern crate rand;

use tokio_threadpool::*;

use futures::future::{lazy, poll_fn};
use futures::*;
use rand::*;

use std::sync::atomic::Ordering::*;
use std::sync::atomic::*;
use std::sync::*;
use std::thread;
use std::time::Duration;

#[test]
fn basic() {
    let _ = ::env_logger::try_init();

    let pool = Builder::new().pool_size(1).max_blocking(1).build();

    let (tx1, rx1) = mpsc::channel();
    let (tx2, rx2) = mpsc::channel();

    pool.spawn(lazy(move || {
        let res = blocking(|| {
            let v = rx1.recv().unwrap();
            tx2.send(v).unwrap();
        })
        .unwrap();

        assert!(res.is_ready());
        Ok(().into())
    }));

    pool.spawn(lazy(move || {
        tx1.send(()).unwrap();
        Ok(().into())
    }));

    rx2.recv().unwrap();
}

#[test]
fn notify_task_on_capacity() {
    const BLOCKING: usize = 10;

    let pool = Builder::new().pool_size(1).max_blocking(1).build();

    let rem = Arc::new(AtomicUsize::new(BLOCKING));
    let (tx, rx) = mpsc::channel();

    for _ in 0..BLOCKING {
        let rem = rem.clone();
        let tx = tx.clone();

        pool.spawn(lazy(move || {
            poll_fn(move || {
                blocking(|| {
                    thread::sleep(Duration::from_millis(100));
                    let prev = rem.fetch_sub(1, Relaxed);

                    if prev == 1 {
                        tx.send(()).unwrap();
                    }
                })
                .map_err(|e| panic!("blocking err {:?}", e))
            })
        }));
    }

    rx.recv().unwrap();

    assert_eq!(0, rem.load(Relaxed));
}

#[test]
fn capacity_is_use_it_or_lose_it() {
    use futures::sync::oneshot;
    use futures::task::Task;
    use futures::Async::*;
    use futures::*;

    // TODO: Run w/ bigger pool size

    let pool = Builder::new().pool_size(1).max_blocking(1).build();

    let (tx1, rx1) = mpsc::channel();
    let (tx2, rx2) = oneshot::channel();
    let (tx3, rx3) = mpsc::channel();
    let (tx4, rx4) = mpsc::channel();

    // First, fill the blocking capacity
    pool.spawn(lazy(move || {
        poll_fn(move || {
            blocking(|| {
                rx1.recv().unwrap();
            })
            .map_err(|_| panic!())
        })
    }));

    pool.spawn(lazy(move || {
        rx2.map_err(|_| panic!()).and_then(|task: Task| {
            poll_fn(move || {
                blocking(|| {
                    // Notify the other task
                    task.notify();

                    // Block until woken
                    rx3.recv().unwrap();
                })
                .map_err(|_| panic!())
            })
        })
    }));

    // Spawn a future that will try to block, get notified, then not actually
    // use the blocking
    let mut i = 0;
    let mut tx2 = Some(tx2);

    pool.spawn(lazy(move || {
        poll_fn(move || {
            match i {
                0 => {
                    i = 1;

                    let res = blocking(|| unreachable!()).map_err(|_| panic!());

                    assert!(res.unwrap().is_not_ready());

                    // Unblock the first blocker
                    tx1.send(()).unwrap();

                    return Ok(NotReady);
                }
                1 => {
                    i = 2;

                    // Skip blocking, and notify the second task that it should
                    // start blocking
                    let me = task::current();
                    tx2.take().unwrap().send(me).unwrap();

                    return Ok(NotReady);
                }
                2 => {
                    let res = blocking(|| unreachable!()).map_err(|_| panic!());

                    assert!(res.unwrap().is_not_ready());

                    // Unblock the first blocker
                    tx3.send(()).unwrap();
                    tx4.send(()).unwrap();
                    Ok(().into())
                }
                _ => unreachable!(),
            }
        })
    }));

    rx4.recv().unwrap();
}

#[test]
fn blocking_thread_does_not_take_over_shutdown_worker_thread() {
    let pool = Builder::new().pool_size(2).max_blocking(1).build();

    let (enter_tx, enter_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    let (try_tx, try_rx) = mpsc::channel();

    let exited = Arc::new(AtomicBool::new(false));

    {
        let exited = exited.clone();

        pool.spawn(lazy(move || {
            poll_fn(move || {
                blocking(|| {
                    enter_tx.send(()).unwrap();
                    exit_rx.recv().unwrap();
                    exited.store(true, Relaxed);
                })
                .map_err(|_| panic!())
            })
        }));
    }

    // Wait for the task to block
    let _ = enter_rx.recv().unwrap();

    // Spawn another task that attempts to block
    pool.spawn(lazy(move || {
        poll_fn(move || {
            let res = blocking(|| {}).unwrap();

            assert_eq!(res.is_ready(), exited.load(Relaxed));

            try_tx.send(res.is_ready()).unwrap();

            Ok(res)
        })
    }));

    // Wait for the second task to try to block (and not be ready).
    let res = try_rx.recv().unwrap();
    assert!(!res);

    // Unblock the first task
    exit_tx.send(()).unwrap();

    // Wait for the second task to successfully block.
    let res = try_rx.recv().unwrap();
    assert!(res);

    drop(pool);
}

#[test]
fn blocking_one_time_gets_capacity_for_multiple_blocks() {
    const ITER: usize = 1;
    const BLOCKING: usize = 2;

    for _ in 0..ITER {
        let pool = Builder::new().pool_size(4).max_blocking(1).build();

        let rem = Arc::new(AtomicUsize::new(BLOCKING));
        let (tx, rx) = mpsc::channel();

        for _ in 0..BLOCKING {
            let rem = rem.clone();
            let tx = tx.clone();

            pool.spawn(lazy(move || {
                poll_fn(move || {
                    // First block
                    let res = blocking(|| {
                        thread::sleep(Duration::from_millis(100));
                    })
                    .map_err(|e| panic!("blocking err {:?}", e));

                    try_ready!(res);

                    let res = blocking(|| {
                        thread::sleep(Duration::from_millis(100));
                        let prev = rem.fetch_sub(1, Relaxed);

                        if prev == 1 {
                            tx.send(()).unwrap();
                        }
                    });

                    assert!(res.unwrap().is_ready());

                    Ok(().into())
                })
            }));
        }

        rx.recv().unwrap();

        assert_eq!(0, rem.load(Relaxed));
    }
}

#[test]
fn shutdown() {
    const ITER: usize = 1_000;
    const BLOCKING: usize = 10;

    for _ in 0..ITER {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();

        let pool = {
            let num_inc = num_inc.clone();
            let num_dec = num_dec.clone();

            Builder::new()
                .pool_size(1)
                .max_blocking(BLOCKING)
                .after_start(move || {
                    num_inc.fetch_add(1, Relaxed);
                })
                .before_stop(move || {
                    num_dec.fetch_add(1, Relaxed);
                })
                .build()
        };

        let barrier = Arc::new(Barrier::new(BLOCKING));

        for _ in 0..BLOCKING {
            let barrier = barrier.clone();
            let tx = tx.clone();

            pool.spawn(lazy(move || {
                let res = blocking(|| {
                    barrier.wait();
                    Ok::<_, ()>(())
                })
                .unwrap();

                tx.send(()).unwrap();

                assert!(res.is_ready());
                Ok(().into())
            }));
        }

        for _ in 0..BLOCKING {
            rx.recv().unwrap();
        }

        // Shutdown
        drop(pool);

        assert_eq!(11, num_inc.load(Relaxed));
        assert_eq!(11, num_dec.load(Relaxed));
    }
}

#[derive(Debug, Copy, Clone)]
enum Sleep {
    Skip,
    Yield,
    Rand,
    Fixed(Duration),
}

#[test]
fn hammer() {
    use self::Sleep::*;

    const ITER: usize = 5;

    let combos = [
        (2, 4, 1_000, Skip),
        (2, 4, 1_000, Yield),
        (2, 4, 100, Rand),
        (2, 4, 100, Fixed(Duration::from_millis(3))),
        (2, 4, 100, Fixed(Duration::from_millis(12))),
    ];

    for &(size, max_blocking, n, sleep) in &combos {
        for _ in 0..ITER {
            let pool = Builder::new()
                .pool_size(size)
                .max_blocking(max_blocking)
                .build();

            let cnt_task = Arc::new(AtomicUsize::new(0));
            let cnt_block = Arc::new(AtomicUsize::new(0));

            for _ in 0..n {
                let cnt_task = cnt_task.clone();
                let cnt_block = cnt_block.clone();

                pool.spawn(lazy(move || {
                    cnt_task.fetch_add(1, Relaxed);

                    poll_fn(move || {
                        blocking(|| {
                            match sleep {
                                Skip => {}
                                Yield => {
                                    thread::yield_now();
                                }
                                Rand => {
                                    let ms = thread_rng().gen_range(3, 12);
                                    thread::sleep(Duration::from_millis(ms));
                                }
                                Fixed(dur) => {
                                    thread::sleep(dur);
                                }
                            }

                            cnt_block.fetch_add(1, Relaxed);
                        })
                        .map_err(|_| panic!())
                    })
                }));
            }

            // Wait for the work to complete
            pool.shutdown_on_idle().wait().unwrap();

            assert_eq!(n, cnt_task.load(Relaxed));
            assert_eq!(n, cnt_block.load(Relaxed));
        }
    }
}
