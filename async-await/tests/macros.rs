#![feature(await_macro, async_await)]

use tokio::async_wait;
use tokio::timer::Delay;
use std::time::{Duration, Instant};

#[tokio::test]
async fn success_no_async() {
    assert!(true);
}

#[tokio::test]
#[should_panic]
async fn fail_no_async() {
    assert!(false);
}

#[tokio::test]
async fn use_timer() {
    let when = Instant::now() + Duration::from_millis(10);
    async_wait!(Delay::new(when));
}
