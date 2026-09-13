#![warn(rust_2018_idioms)]
#![cfg(all(feature = "time", feature = "sync", feature = "io-util"))]

use tokio::time;
use tokio_stream::{Stream, StreamExt};
use tokio_test::*;

use std::time::Duration;

#[test]
fn throttle_can_be_created_outside_runtime() {
    let _stream = futures::stream::iter([1, 2]).throttle(Duration::from_millis(1));
}

#[tokio::test]
async fn usage() {
    time::pause();

    let mut stream = task::spawn(futures::stream::repeat(()).throttle(Duration::from_millis(100)));

    assert_ready!(stream.poll_next());
    assert_pending!(stream.poll_next());

    time::advance(Duration::from_millis(90)).await;

    assert_pending!(stream.poll_next());

    time::advance(Duration::from_millis(101)).await;

    assert!(stream.is_woken());

    assert_ready!(stream.poll_next());
}

#[tokio::test]
async fn duration_max_does_not_overflow() {
    let mut stream = task::spawn(futures::stream::iter([1]).throttle(Duration::MAX));

    assert_ready_eq!(stream.poll_next(), Some(1));
}

#[tokio::test]
async fn size_hint() {
    let stream = futures::stream::iter([1, 2, 3]).throttle(Duration::from_secs(1));

    assert_eq!(stream.size_hint(), (3, Some(3)));
}
