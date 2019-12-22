#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::time::{self, Duration, Instant};
use tokio_test::{assert_pending, assert_ready_eq, task};

use std::task::Poll;

#[tokio::test]
#[should_panic]
async fn interval_zero_duration() {
    let _ = time::interval_at(Instant::now(), ms(0));
}

#[tokio::test]
async fn usage() {
    time::pause();

    let start = Instant::now();

    // TODO: Skip this
    time::advance(ms(1)).await;

    let mut i = task::spawn(time::interval_at(start, ms(300)));

    assert_ready_eq!(poll_next(&mut i), start);
    assert_pending!(poll_next(&mut i));

    time::advance(ms(100)).await;
    assert_pending!(poll_next(&mut i));

    time::advance(ms(200)).await;
    assert_ready_eq!(poll_next(&mut i), start + ms(300));
    assert_pending!(poll_next(&mut i));

    time::advance(ms(400)).await;
    assert_ready_eq!(poll_next(&mut i), start + ms(600));
    assert_pending!(poll_next(&mut i));

    time::advance(ms(500)).await;
    assert_ready_eq!(poll_next(&mut i), start + ms(900));
    assert_ready_eq!(poll_next(&mut i), start + ms(1200));
    assert_pending!(poll_next(&mut i));
}

#[tokio::test]
async fn usage_stream() {
    use tokio::stream::StreamExt;

    let start = Instant::now();
    let mut interval = time::interval(ms(10));

    for _ in 0..3 {
        interval.next().await.unwrap();
    }

    assert!(start.elapsed() > ms(20));
}

fn poll_next(interval: &mut task::Spawn<time::Interval>) -> Poll<Instant> {
    interval.enter(|cx, mut interval| interval.poll_tick(cx))
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
