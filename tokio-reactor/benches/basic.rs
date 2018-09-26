#![feature(test)]
#![deny(warnings)]

extern crate futures;
extern crate mio;
extern crate num_cpus;
extern crate test;
extern crate tokio;
extern crate tokio_reactor;

use std::sync::mpsc;

use futures::{future, Async};
use self::test::Bencher;
use tokio_reactor::Registration;

const NUM_YIELD: usize = 1_000;
const TASKS_PER_CPU: usize = 50;

#[bench]
fn notify_many(b: &mut Bencher) {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let tasks = TASKS_PER_CPU * num_cpus::get();
    let (tx, rx) = mpsc::channel();

    b.iter(|| {
        for _ in 0..tasks {
            let (r, s) = mio::Registration::new2();
            let registration = Registration::new();
            registration.register(&r).unwrap();

            let mut rem = NUM_YIELD;
            let mut r = Some(r);
            let tx = tx.clone();

            rt.spawn(future::poll_fn(move || {
                loop {
                    let is_ready = registration.poll_read_ready().unwrap().is_ready();

                    if is_ready {
                        rem -= 1;

                        if rem == 0 {
                            r.take().unwrap();
                            tx.send(()).unwrap();
                            return Ok(Async::Ready(()));
                        }
                    } else {
                        s.set_readiness(mio::Ready::readable()).unwrap();
                        return Ok(Async::NotReady);
                    }
                }
            }));
        }

        for _ in 0..tasks {
            rx.recv().unwrap();
        }
    });
}
