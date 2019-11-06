#![warn(rust_2018_idioms)]

use tokio::runtime;
use tokio::timer::clock::Clock;
use tokio::timer::*;

use std::sync::mpsc;
use std::time::{Duration, Instant};

struct MockNow(Instant);

impl tokio::timer::clock::Now for MockNow {
    fn now(&self) -> Instant {
        self.0
    }
}

#[test]
fn clock_and_timer_concurrent() {
    let when = Instant::now() + Duration::from_millis(5_000);
    let clock = Clock::new_with_now(MockNow(when));

    let mut rt = runtime::Builder::new()
        .thread_pool()
        .clock(clock)
        .build()
        .unwrap();

    let (tx, rx) = mpsc::channel();

    rt.block_on(async move {
        tokio::spawn(async move {
            delay(when).await;
            assert!(Instant::now() < when);
            tx.send(()).unwrap();
        })
    });

    rx.recv().unwrap();
}

#[test]
fn clock_and_timer_single_threaded() {
    let when = Instant::now() + Duration::from_millis(5_000);
    let clock = Clock::new_with_now(MockNow(when));

    let mut rt = runtime::Builder::new()
        .current_thread()
        .clock(clock)
        .build()
        .unwrap();

    rt.block_on(async move {
        delay(when).await;
        assert!(Instant::now() < when);
    });
}

#[test]
fn mocked_clock_delay_for() {
    tokio_test::clock::mock(|handle| {
        let mut f = tokio_test::task::spawn(delay_for(Duration::from_millis(1)));
        tokio_test::assert_pending!(f.poll());
        handle.advance(Duration::from_millis(1));
        tokio_test::assert_ready!(f.poll());
    });
}
