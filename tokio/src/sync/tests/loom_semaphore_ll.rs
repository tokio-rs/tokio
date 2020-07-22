use crate::sync::semaphore_ll::*;

use futures::future::poll_fn;
use loom::future::block_on;
use loom::thread;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::task::Poll::Ready;
use std::task::{Context, Poll};

#[test]
fn basic_usage() {
    const NUM: usize = 2;

    struct Actor {
        waiter: Permit,
        shared: Arc<Shared>,
    }

    struct Shared {
        semaphore: Semaphore,
        active: AtomicUsize,
    }

    impl Future for Actor {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            let me = &mut *self;

            ready!(me.waiter.poll_acquire(cx, 1, &me.shared.semaphore)).unwrap();

            let actual = me.shared.active.fetch_add(1, SeqCst);
            assert!(actual <= NUM - 1);

            let actual = me.shared.active.fetch_sub(1, SeqCst);
            assert!(actual <= NUM);

            me.waiter.release(1, &me.shared.semaphore);

            Ready(())
        }
    }

    loom::model(|| {
        let shared = Arc::new(Shared {
            semaphore: Semaphore::new(NUM),
            active: AtomicUsize::new(0),
        });

        for _ in 0..NUM {
            let shared = shared.clone();

            thread::spawn(move || {
                block_on(Actor {
                    waiter: Permit::new(),
                    shared,
                });
            });
        }

        block_on(Actor {
            waiter: Permit::new(),
            shared,
        });
    });
}

#[test]
fn release() {
    loom::model(|| {
        let semaphore = Arc::new(Semaphore::new(1));

        {
            let semaphore = semaphore.clone();
            thread::spawn(move || {
                let mut permit = Permit::new();

                block_on(poll_fn(|cx| permit.poll_acquire(cx, 1, &semaphore))).unwrap();

                permit.release(1, &semaphore);
            });
        }

        let mut permit = Permit::new();

        block_on(poll_fn(|cx| permit.poll_acquire(cx, 1, &semaphore))).unwrap();

        permit.release(1, &semaphore);
    });
}

#[test]
fn basic_closing() {
    const NUM: usize = 2;

    loom::model(|| {
        let semaphore = Arc::new(Semaphore::new(1));

        for _ in 0..NUM {
            let semaphore = semaphore.clone();

            thread::spawn(move || {
                let mut permit = Permit::new();

                for _ in 0..2 {
                    block_on(poll_fn(|cx| {
                        permit.poll_acquire(cx, 1, &semaphore).map_err(|_| ())
                    }))?;

                    permit.release(1, &semaphore);
                }

                Ok::<(), ()>(())
            });
        }

        semaphore.close();
    });
}

#[test]
fn concurrent_close() {
    const NUM: usize = 3;

    loom::model(|| {
        let semaphore = Arc::new(Semaphore::new(1));

        for _ in 0..NUM {
            let semaphore = semaphore.clone();

            thread::spawn(move || {
                let mut permit = Permit::new();

                block_on(poll_fn(|cx| {
                    permit.poll_acquire(cx, 1, &semaphore).map_err(|_| ())
                }))?;

                permit.release(1, &semaphore);

                semaphore.close();

                Ok::<(), ()>(())
            });
        }
    });
}

#[test]
fn batch() {
    let mut b = loom::model::Builder::new();
    b.preemption_bound = Some(1);

    b.check(|| {
        let semaphore = Arc::new(Semaphore::new(10));
        let active = Arc::new(AtomicUsize::new(0));
        let mut ths = vec![];

        for _ in 0..2 {
            let semaphore = semaphore.clone();
            let active = active.clone();

            ths.push(thread::spawn(move || {
                let mut permit = Permit::new();

                for n in &[4, 10, 8] {
                    block_on(poll_fn(|cx| permit.poll_acquire(cx, *n, &semaphore))).unwrap();

                    active.fetch_add(*n as usize, SeqCst);

                    let num_active = active.load(SeqCst);
                    assert!(num_active <= 10);

                    thread::yield_now();

                    active.fetch_sub(*n as usize, SeqCst);

                    permit.release(*n, &semaphore);
                }
            }));
        }

        for th in ths.into_iter() {
            th.join().unwrap();
        }

        assert_eq!(10, semaphore.available_permits());
    });
}
