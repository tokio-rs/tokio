#![warn(rust_2018_idioms)]

use std::time::Instant;
use tokio_timer::clock;
use tokio_timer::clock::*;

struct ConstNow(Instant);

impl Now for ConstNow {
    fn now(&self) -> Instant {
        self.0
    }
}

#[test]
fn default_clock() {
    let a = Instant::now();
    let b = clock::now();
    let c = Clock::new().now();

    assert!(a <= b);
    assert!(b <= c);
}

#[test]
fn custom_clock() {
    let now = ConstNow(Instant::now());
    let clock = Clock::new_with_now(now);

    let a = Instant::now();
    let b = clock.now();

    assert!(b <= a);
}

#[test]
fn execution_context() {
    let now = ConstNow(Instant::now());
    let clock = Clock::new_with_now(now);

    with_default(&clock, || {
        let a = Instant::now();
        let b = clock::now();

        assert!(b <= a);
    });
}
