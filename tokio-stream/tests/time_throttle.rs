#![warn(rust_2018_idioms)]
#![cfg(all(feature = "time", feature = "sync", feature = "io-util"))]

use futures_core::FusedStream;
use tokio::time;
use tokio_stream::{Stream, StreamExt};
use tokio_test::*;

use std::time::Duration;

#[test]
fn throttle_can_be_created_outside_runtime() {
    let _stream = futures::stream::iter([1, 2]).throttle(Duration::from_millis(1));
}

#[test]
fn zero_duration_does_not_require_time_driver() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async {
        let stream = futures::stream::iter([1, 2]).throttle(Duration::from_millis(0));
        tokio::pin!(stream);

        assert_eq!(stream.next().await, Some(1));
        assert_eq!(stream.next().await, Some(2));
        assert_eq!(stream.next().await, None);
    });
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

#[tokio::test(start_paused = true)]
async fn is_terminated() {

    let stream = tokio_stream::once(1).throttle(Duration::from_millis(100));
    tokio::pin!(stream);

    assert!(!stream.as_ref().get_ref().is_terminated());
    assert_eq!(stream.next().await, Some(1));

    // The inner stream is already terminated, but the throttle delay is still pending.
    assert!(stream.as_ref().get_ref().get_ref().is_terminated());
    assert!(!stream.as_ref().get_ref().is_terminated());

    time::sleep(Duration::from_millis(101)).await;

    // After the delay has elapsed, the stream yields None and is terminated.
    assert_eq!(stream.next().await, None);
    assert!(stream.as_ref().get_ref().is_terminated());
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn is_terminated_zero_duration() {
    let stream = tokio_stream::once(1).throttle(Duration::from_millis(0));
    tokio::pin!(stream);

    assert!(!stream.as_ref().get_ref().is_terminated());
    assert_eq!(stream.next().await, Some(1));
    assert!(stream.as_ref().get_ref().get_ref().is_terminated());
    assert!(stream.as_ref().get_ref().is_terminated());
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn is_terminated_empty() {
    let stream = tokio_stream::empty::<i32>().throttle(Duration::from_millis(100));
    tokio::pin!(stream);

    assert!(stream.as_ref().get_ref().is_terminated());
    assert_eq!(stream.next().await, None);
    assert!(stream.as_ref().get_ref().is_terminated());
}
