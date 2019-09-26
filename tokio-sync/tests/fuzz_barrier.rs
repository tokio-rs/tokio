#![warn(rust_2018_idioms)]

#[macro_use]
extern crate loom;

#[path = "../src/barrier.rs"]
#[allow(warnings)]
mod barrier;

#[path = "../src/semaphore.rs"]
#[allow(warnings)]
mod semaphore;

#[path = "../src/watch.rs"]
#[allow(warnings)]
mod watch;

use crate::barrier::*;

//use futures_core::ready;
//use futures_util::future::poll_fn;
use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;
//use std::future::Future;
//use std::pin::Pin;
//use std::sync::atomic::AtomicUsize;
//use std::sync::atomic::Ordering::SeqCst;
//use std::task::Poll::Ready;
//use std::task::{Context, Poll};

#[test]
fn basic_usage() {
    const NUM: usize = 2;

    loom::model(|| {
        let shared = Arc::new(Barrier::new(NUM));

        let ws: Vec<_> = (1..NUM)
            .map(|_| {
                let shared = Arc::clone(&shared);

                thread::spawn(move || block_on(shared.wait()))
            })
            .collect();

        let mut leaders = 0;
        if block_on(shared.wait()).is_leader() == true {
            leaders += 1;
        }
        for w in ws {
            if w.join().unwrap().is_leader() == true {
                leaders += 1;
            }
        }
        assert_eq!(leaders, 1);
    });
}
