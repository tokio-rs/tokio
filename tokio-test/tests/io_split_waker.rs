#![warn(rust_2018_idioms)]

use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_test::io::Builder;

// Regression tests for #7445.
//
// `poll_read` and `poll_write` drive the same `Sleep`, and a timer only stores
// the waker of its most recent poll. Whichever half polled it last used to
// evict the other half's waker, leaving that half permanently unwoken.
//
// Both tests assert on elapsed time rather than merely on completion: wrapping
// the stranded operation in a `timeout` masks the bug, because the timeout's
// own timer wakes the task and lets the operation re-poll. The `timeout` calls
// here are only guards, so that a regression fails the suite instead of
// hanging it forever.

/// The write half must still be woken when the read half polled the shared
/// `Sleep` last.
#[tokio::test]
async fn split_wait_does_not_lose_writer_waker() {
    let socket = Builder::new()
        .wait(Duration::from_millis(10))
        .write([0].as_slice())
        .build();

    let (mut recv, mut send) = tokio::io::split(socket);

    // The read half polls the shared `Sleep` after the write half parks on it.
    let reader = tokio::spawn(async move {
        let _ = recv.read_u8().await;
    });

    let start = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), send.write_u8(0))
        .await
        .expect("the write half was never woken")
        .expect("write failed");
    let elapsed = start.elapsed();

    reader.abort();

    assert!(
        elapsed < Duration::from_millis(500),
        "the write half lost its waker: a 10ms wait took {elapsed:?}"
    );
}

/// The mirror case: the read half must still be woken when the write half
/// polled the shared `Sleep` last.
#[tokio::test]
async fn split_wait_does_not_lose_reader_waker() {
    let socket = Builder::new()
        .wait(Duration::from_millis(10))
        .read(b"z")
        .build();

    let (mut recv, mut send) = tokio::io::split(socket);

    let start = Instant::now();

    // Park the read half on the shared `Sleep` first.
    let reader = tokio::spawn(async move { recv.read_u8().await });
    tokio::task::yield_now().await;

    // Then poll the write half, which drives the same `Sleep` and takes over
    // the timer's single waker slot. This write never completes (the script
    // has no `write` action), it only needs to poll the shared timer.
    let _ = tokio::time::timeout(Duration::from_millis(50), send.write_u8(0)).await;

    let byte = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .expect("the read half was never woken")
        .expect("the reader task panicked")
        .expect("read failed");
    let elapsed = start.elapsed();

    assert_eq!(byte, b'z');
    assert!(
        elapsed < Duration::from_millis(500),
        "the read half lost its waker: a 10ms wait took {elapsed:?}"
    );
}
