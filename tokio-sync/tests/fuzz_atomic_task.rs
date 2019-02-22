#![deny(warnings)]

extern crate futures;
#[macro_use]
extern crate loom;

#[allow(dead_code)]
#[path = "../src/task/atomic_task.rs"]
mod atomic_task;

use atomic_task::AtomicTask;

use loom::futures::block_on;
use loom::sync::atomic::AtomicUsize;
use loom::thread;

use futures::future::poll_fn;
use futures::Async;

use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

struct Chan {
    num: AtomicUsize,
    task: AtomicTask,
}

#[test]
fn basic_notification() {
    const NUM_NOTIFY: usize = 2;

    loom::fuzz(|| {
        let chan = Arc::new(Chan {
            num: AtomicUsize::new(0),
            task: AtomicTask::new(),
        });

        for _ in 0..NUM_NOTIFY {
            let chan = chan.clone();

            thread::spawn(move || {
                chan.num.fetch_add(1, Relaxed);
                chan.task.notify();
            });
        }

        block_on(poll_fn(move || {
            chan.task.register();

            if NUM_NOTIFY == chan.num.load(Relaxed) {
                return Ok(Async::Ready(()));
            }

            Ok::<_, ()>(Async::NotReady)
        }))
        .unwrap();
    });
}
