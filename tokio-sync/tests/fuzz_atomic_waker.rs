#![deny(warnings, rust_2018_idioms)]

#[macro_use]
extern crate loom;

#[allow(dead_code)]
#[path = "../src/task/atomic_waker.rs"]
mod atomic_waker;
use crate::atomic_waker::AtomicWaker;

use async_util::future::poll_fn;
use loom::futures::block_on;
use loom::sync::atomic::AtomicUsize;
use loom::thread;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::task::Poll::{Pending, Ready};

struct Chan {
    num: AtomicUsize,
    task: AtomicWaker,
}

#[test]
fn basic_notification() {
    const NUM_NOTIFY: usize = 2;

    loom::model(|| {
        let chan = Arc::new(Chan {
            num: AtomicUsize::new(0),
            task: AtomicWaker::new(),
        });

        for _ in 0..NUM_NOTIFY {
            let chan = chan.clone();

            thread::spawn(move || {
                chan.num.fetch_add(1, Relaxed);
                chan.task.wake();
            });
        }

        block_on(poll_fn(move |cx| {
            chan.task.register_by_ref(cx.waker());

            if NUM_NOTIFY == chan.num.load(Relaxed) {
                return Ready(());
            }

            Pending
        }));
    });
}
