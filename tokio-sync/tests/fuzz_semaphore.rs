#[macro_use]
extern crate futures;
#[macro_use]
extern crate loom;

#[path = "../src/semaphore.rs"]
#[allow(warnings)]
mod semaphore;

use semaphore::*;

use futures::{Future, Async, Poll};
use loom::futures::spawn;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;

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
        try_ready!(self.waiter.poll_acquire(&self.shared.semaphore));

        let actual = self.shared.active.fetch_add(1, SeqCst);
        assert!(actual <= NUM-1);

        let actual = self.shared.active.fetch_sub(1, SeqCst);
        assert!(actual <= NUM);

        self.waiter.release(&self.shared.semaphore);

        Ok(Async::Ready(()))
    }
}

#[test]
fn smoke() {
    loom::fuzz_future(|| {
        let shared = Arc::new(Shared {
            semaphore: Semaphore::new(NUM),
            active: AtomicUsize::new(0),
        });

        for _ in 0..NUM {
            spawn(Actor {
                waiter: Permit::new(),
                shared: shared.clone(),
            });
        }

        Actor {
            waiter: Permit::new(),
            shared
        }
    });
}
