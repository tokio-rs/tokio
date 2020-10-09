#![warn(rust_2018_idioms)]

use tokio::time::{sleep_until, Duration, Instant};
use tokio_test::block_on;

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
fn test_sleep() {
    let deadline = Instant::now() + Duration::from_millis(100);

    block_on(async {
        sleep_until(deadline).await;
    });
}
