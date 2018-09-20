extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate env_logger;

use tokio::prelude::*;
use tokio::timer::*;

use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn timer_with_runtime() {
    let _ = env_logger::try_init();

    let when = Instant::now() + Duration::from_millis(100);
    let (tx, rx) = mpsc::channel();

    tokio::run({
        Delay::new(when)
            .map_err(|e| panic!("unexpected error; err={:?}", e))
            .and_then(move |_| {
                assert!(Instant::now() >= when);
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.recv().unwrap();
}

#[test]
fn starving() {
    use futures::{task, Poll, Async};

    let _ = env_logger::try_init();

    struct Starve(Delay, u64);

    impl Future for Starve {
        type Item = u64;
        type Error = ();

        fn poll(&mut self) -> Poll<Self::Item, ()> {
            if self.0.poll().unwrap().is_ready() {
                return Ok(self.1.into());
            }

            self.1 += 1;

            task::current().notify();

            Ok(Async::NotReady)
        }
    }

    let when = Instant::now() + Duration::from_millis(20);
    let starve = Starve(Delay::new(when), 0);

    let (tx, rx) = mpsc::channel();

    tokio::run({
        starve
            .and_then(move |_ticks| {
                assert!(Instant::now() >= when);
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.recv().unwrap();
}

#[test]
fn deadline() {
    use futures::future;

    let _ = env_logger::try_init();

    let when = Instant::now() + Duration::from_millis(20);
    let (tx, rx) = mpsc::channel();

    #[allow(deprecated)]
    tokio::run({
        future::empty::<(), ()>()
            .deadline(when)
            .then(move |res| {
                assert!(res.is_err());
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.recv().unwrap();
}

#[test]
fn timeout() {
    use futures::future;

    let _ = env_logger::try_init();

    let (tx, rx) = mpsc::channel();

    tokio::run({
        future::empty::<(), ()>()
            .timeout(Duration::from_millis(20))
            .then(move |res| {
                assert!(res.is_err());
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.recv().unwrap();
}
