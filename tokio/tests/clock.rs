#![feature(async_await)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "default")]

use tokio::runtime::{self, current_thread};
use tokio::timer::*;
use tokio_timer;
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
    let when = Instant::now() + Duration::from_millis(5_000);
    let clock = Clock::new_with_now(MockNow(when));

    let rt = runtime::Builder::new().clock(clock).build().unwrap();

    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        delay(when).await;
        assert!(Instant::now() < when);
        tx.send(()).unwrap();
    });

    rx.recv().unwrap();
}

#[test]
fn clock_and_timer_single_threaded() {
    let when = Instant::now() + Duration::from_millis(5_000);
    let clock = Clock::new_with_now(MockNow(when));

    let mut rt = current_thread::Builder::new().clock(clock).build().unwrap();

    rt.block_on(async move {
        delay(when).await;
        assert!(Instant::now() < when);
    });
}
