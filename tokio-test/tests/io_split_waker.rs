#![warn(rust_2018_idioms)]

use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_test::io::Builder;

/// Regression test for #7445.
///
/// `poll_read` and `poll_write` drive the same `Sleep`, and a timer only stores
/// the waker of its most recent poll. Whichever half polled it last used to
/// evict the other half's waker, leaving that half permanently unwoken.
///
/// Note this asserts on elapsed time rather than simply on completion: wrapping
/// the write in a `timeout` masks the bug, because the timeout's own timer wakes
/// the task and lets the write re-poll. The `timeout` here is only a guard so a
/// regression fails the suite instead of hanging it forever.
#[tokio::test]
async fn split_wait_does_not_lose_writer_waker() {
    let socket = Builder::new()
        .wait(Duration::from_millis(10))
        .write([0].as_slice())
        .build();

    let (mut recv, mut send) = tokio::io::split(socket);

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

/// The same scenario with the roles reversed: the read half must still be woken
/// when the write half is the one that polled the shared `Sleep` last.
#[tokio::test]
async fn split_wait_does_not_lose_reader_waker() {
    let socket = Builder::new()
        .wait(Duration::from_millis(10))
        .read(b"z")
        .build();

    let (mut recv, send) = tokio::io::split(socket);

    let start = Instant::now();
    let byte = tokio::time::timeout(Duration::from_secs(5), recv.read_u8())
        .await
        .expect("the read half was never woken")
        .expect("read failed");
    let elapsed = start.elapsed();

    drop(send);

    assert_eq!(byte, b'z');
    assert!(
        elapsed < Duration::from_millis(500),
        "the read half lost its waker: a 10ms wait took {elapsed:?}"
    );
}
