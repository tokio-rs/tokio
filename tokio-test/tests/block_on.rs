#![warn(rust_2018_idioms)]
#![feature(async_await)]

use std::time::{Duration, Instant};
use tokio_test::block_on;
use tokio_timer::Delay;

#[test]
fn async_block() {
    assert_eq!(4, block_on(async { 4 }));
}

async fn five() -> u8 {
    5
}

#[test]
fn async_fn() {
    assert_eq!(5, block_on(five()));
}

#[test]
fn delay() {
    let deadline = Instant::now() + Duration::from_millis(100);
    let delay = Delay::new(deadline);

    assert_eq!((), block_on(delay));
}
