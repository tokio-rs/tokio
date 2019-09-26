#![warn(rust_2018_idioms)]

#[macro_use]
extern crate loom;

#[path = "../src/watch.rs"]
#[allow(warnings)]
mod watch;

#[path = "../src/semaphore.rs"]
#[allow(warnings)]
mod semaphore;

//use futures_core::ready;
//use futures_util::future::poll_fn;
use loom::future::block_on;
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
        let (_tx, mut rx) = watch::channel("one");

        let ws: Vec<_> = (0..NUM)
            .map(|_| {
                let mut rx = rx.clone();

                thread::spawn(move || *block_on(rx.recv_ref()).unwrap())
            })
            .collect();

        for w in ws {
            assert_eq!(w.join().unwrap(), "one");
        }
        assert_eq!(*block_on(rx.recv_ref()).unwrap(), "one");
    });
}

#[test]
fn progress() {
    loom::model(|| {
        let (tx, mut rx) = watch::channel("one");

        tx.broadcast("two").unwrap();

        let jh1 = thread::spawn(move || {
            tx.broadcast("three").unwrap();
            thread::yield_now();
            tx.broadcast("four").unwrap();
            tx
        });

        let mut rx1 = rx.clone();
        let jh2 = thread::spawn(move || {
            let mut got = 2;
            loop {
                match *block_on(rx1.recv_ref()).unwrap() {
                    "two" if got == 2 => {}
                    "three" if got == 2 || got == 3 => {
                        got = 3;
                    }
                    "four" if got == 3 => {
                        got = 4;
                    }
                    "four" if got == 4 => {
                        break;
                    }
                    r => {
                        panic!("got unexpected {} when previous was {}", r, got);
                    }
                }
            }
        });

        assert_ne!(*block_on(rx.recv_ref()).unwrap(), "one");
        let _tx = jh1.join().unwrap();
        assert_eq!(*block_on(rx.recv_ref()).unwrap(), "four");
        jh2.join().unwrap();
    });
}
