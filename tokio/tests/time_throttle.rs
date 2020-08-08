#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::stream::StreamExt;
use tokio::time;
use tokio_test::*;

use std::time::Duration;

#[tokio::test]
async fn usage() {
    time::pause();

    let stream_repeat = futures::stream::repeat(());

    let mut stream = task::spawn(stream_repeat.throttle(Duration::from_millis(100)));

    assert_ready!(stream.poll_next());
    assert_pending!(stream.poll_next());

    time::advance(Duration::from_millis(90)).await;

    assert_pending!(stream.poll_next());

    time::advance(Duration::from_millis(101)).await;

    assert!(stream.is_woken());

    assert_ready!(stream.poll_next());
}
