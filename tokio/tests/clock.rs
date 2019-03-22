extern crate env_logger;
extern crate futures;
extern crate tokio;
extern crate tokio_timer;

use tokio::prelude::*;
use tokio::runtime::{self, current_thread};
use tokio::timer::*;
use tokio_timer::clock::Clock;

use std::sync::mpsc;
use std::time::{Duration, Instant};

struct MockNow(Instant);

impl tokio_timer::clock::Now for MockNow {
    fn now(&self) -> Instant {
        self.0
    }
}

#[test]
fn clock_and_timer_concurrent() {
    let _ = env_logger::try_init();

    let when = Instant::now() + Duration::from_millis(5_000);
    let clock = Clock::new_with_now(MockNow(when));

    let mut rt = runtime::Builder::new().clock(clock).build().unwrap();

    let (tx, rx) = mpsc::channel();

    rt.spawn({
        Delay::new(when)
            .map_err(|e| panic!("unexpected error; err={:?}", e))
            .and_then(move |_| {
                assert!(Instant::now() < when);
                tx.send(()).unwrap();
                Ok(())
            })
    });

    rx.recv().unwrap();
}

#[test]
fn clock_and_timer_single_threaded() {
    let _ = env_logger::try_init();

    let when = Instant::now() + Duration::from_millis(5_000);
    let clock = Clock::new_with_now(MockNow(when));

    let mut rt = current_thread::Builder::new().clock(clock).build().unwrap();

    rt.block_on({
        Delay::new(when)
            .map_err(|e| panic!("unexpected error; err={:?}", e))
            .and_then(move |_| {
                assert!(Instant::now() < when);
                Ok(())
            })
    })
    .unwrap();
}
