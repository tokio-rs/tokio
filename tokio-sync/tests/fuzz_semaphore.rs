#[macro_use]
extern crate futures;
#[macro_use]
extern crate loom;

#[path = "../src/semaphore.rs"]
#[allow(warnings)]
mod semaphore;

use semaphore::*;

use futures::{future, Async, Future, Poll};
use loom::futures::block_on;
use loom::thread;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

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
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            try_ready!(self
                .waiter
                .poll_acquire(&self.shared.semaphore)
                .map_err(|_| ()));

            let actual = self.shared.active.fetch_add(1, SeqCst);
            assert!(actual <= NUM - 1);

            let actual = self.shared.active.fetch_sub(1, SeqCst);
            assert!(actual <= NUM);

            self.waiter.release(&self.shared.semaphore);

            Ok(Async::Ready(()))
        }
    }

    loom::fuzz(|| {
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
                })
                .unwrap();
            });
        }

        block_on(Actor {
            waiter: Permit::new(),
            shared,
        })
        .unwrap();
    });
}

#[test]
fn release() {
    loom::fuzz(|| {
        let semaphore = Arc::new(Semaphore::new(1));

        {
            let semaphore = semaphore.clone();
            thread::spawn(move || {
                let mut permit = Permit::new();

                block_on(future::lazy(|| {
                    permit.poll_acquire(&semaphore).unwrap();
                    Ok::<_, ()>(())
                }))
                .unwrap();

                permit.release(&semaphore);
            });
        }

        let mut permit = Permit::new();

        block_on(future::poll_fn(|| permit.poll_acquire(&semaphore))).unwrap();

        permit.release(&semaphore);
    });
}

#[test]
fn basic_closing() {
    const NUM: usize = 2;

    loom::fuzz(|| {
        let semaphore = Arc::new(Semaphore::new(1));

        for _ in 0..NUM {
            let semaphore = semaphore.clone();

            thread::spawn(move || {
                let mut permit = Permit::new();

                for _ in 0..2 {
                    block_on(future::poll_fn(|| {
                        permit.poll_acquire(&semaphore).map_err(|_| ())
                    }))?;
                    permit.release(&semaphore);
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

    loom::fuzz(|| {
        let semaphore = Arc::new(Semaphore::new(1));

        for _ in 0..NUM {
            let semaphore = semaphore.clone();

            thread::spawn(move || {
                let mut permit = Permit::new();

                block_on(future::poll_fn(|| {
                    permit.poll_acquire(&semaphore).map_err(|_| ())
                }))?;

                permit.release(&semaphore);

                semaphore.close();

                Ok::<(), ()>(())
            });
        }
    });
}
