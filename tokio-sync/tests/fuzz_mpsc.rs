extern crate futures;
#[macro_use]
extern crate loom;

macro_rules! if_fuzz {
    ($($t:tt)*) => {
        $($t)*
    }
}

#[path = "../src/mpsc/mod.rs"]
#[allow(warnings)]
mod mpsc;

#[path = "../src/semaphore.rs"]
#[allow(warnings)]
mod semaphore;

use futures::{future::poll_fn, Stream};
use loom::futures::block_on;
use loom::thread;

#[test]
fn closing_tx() {
    loom::fuzz(|| {
        let (mut tx, mut rx) = mpsc::channel(16);

        thread::spawn(move || {
            tx.try_send(()).unwrap();
            drop(tx);
        });

        let v = block_on(poll_fn(|| rx.poll())).unwrap();
        assert!(v.is_some());

        let v = block_on(poll_fn(|| rx.poll())).unwrap();
        assert!(v.is_none());
    });
}
