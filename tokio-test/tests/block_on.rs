#![warn(rust_2018_idioms)]

use tokio::time::{delay_until, Duration, Instant};
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
fn test_delay() {
    let deadline = Instant::now() + Duration::from_millis(100);
    assert_eq!(
        (),
        block_on(async {
            delay_until(deadline).await;
        })
    );
}
