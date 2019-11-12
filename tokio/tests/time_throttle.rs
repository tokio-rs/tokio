#![warn(rust_2018_idioms)]

use tokio::sync::mpsc;
use tokio::time::throttle::Throttle;
use tokio::time::Instant;
use tokio_test::{assert_pending, assert_ready_eq};

use futures::future::poll_fn;
use futures::StreamExt;
use std::task::Poll;
use std::time::Duration;

#[tokio::test]
async fn throttle() {
    let (mut tx, rx) = mpsc::unbounded_channel();
    let mut stream = Throttle::new(rx, ms(1));

    poll_fn(|cx| {
        assert_pending!(stream.poll_next_unpin(cx));
        Poll::Ready(())
    })
    .await;

    for i in 0..3 {
        tx.try_send(i).unwrap();
    }

    drop(tx);

    let mut now = Instant::now();

    while let Some(_) = stream.next().await {
        assert!(Instant::now() >= now);
        now += ms(1);
    }
}

#[tokio::test]
async fn throttle_dur_0() {
    let (mut tx, rx) = mpsc::unbounded_channel();
    let mut stream = Throttle::new(rx, ms(0));

    poll_fn(|cx| {
        assert_pending!(stream.poll_next_unpin(cx));

        for i in 0..3 {
            tx.try_send(i).unwrap();
        }

        Poll::Ready(())
    })
    .await;

    poll_fn(|cx| {
        for i in 0..3 {
            assert_ready_eq!(stream.poll_next_unpin(cx), Some(i), "i = {}", i);
        }

        assert_pending!(stream.poll_next_unpin(cx));

        Poll::Ready(())
    })
    .await;
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
