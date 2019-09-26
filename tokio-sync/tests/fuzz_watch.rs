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
            //eprintln!("broadcast three");
            tx.broadcast("three").unwrap();
            thread::yield_now();
            //eprintln!("broadcast four");
            tx.broadcast("four").unwrap();
            //eprintln!("broadcaster return");
            tx
        });

        let mut rx1 = rx.clone();
        let jh2 = thread::spawn(move || {
            let mut got = 2;
            loop {
                let recv = *block_on(rx1.recv_ref()).unwrap();
                //eprintln!("received {} with got == {}", recv, got);
                match recv {
                    "two" if got == 2 => {}
                    "three" if got == 2 || got == 3 => {
                        got = 3;
                    }
                    "four" if got == 2 || got == 3 => {
                        break;
                    }
                    r => {
                        panic!("got unexpected {} when previous was {}", r, got);
                    }
                }
            }
        });

        thread::yield_now();
        //eprintln!("main try recv");
        // we know we'll wake up since we've never read
        let recvd = *block_on(rx.recv_ref()).unwrap();
        //eprintln!("main joining");
        let _tx = jh1.join().unwrap();
        //eprintln!("main joined");
        // we know we'll wake up as long as we didn't already receive four
        if recvd != "four" {
            assert_eq!(*block_on(rx.recv_ref()).unwrap(), "four");
        }
        //eprintln!("main joining again");
        jh2.join().unwrap();
        //eprintln!("main joined again");
    });
}
