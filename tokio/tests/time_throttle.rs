#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::time::{self, throttle};
use tokio_test::*;

use std::time::Duration;

#[tokio::test]
async fn usage() {
    time::pause();

    let mut stream = task::spawn(throttle(
        Duration::from_millis(100),
        futures::stream::repeat(()),
    ));

    assert_ready!(stream.poll_next());
    assert_pending!(stream.poll_next());

    time::advance(Duration::from_millis(90)).await;

    assert_pending!(stream.poll_next());

    time::advance(Duration::from_millis(101)).await;

    assert!(stream.is_woken());

    assert_ready!(stream.poll_next());
}
