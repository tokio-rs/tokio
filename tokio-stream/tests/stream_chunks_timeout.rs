#![warn(rust_2018_idioms)]

use futures::FutureExt;
use std::error::Error;
use tokio::time;
use tokio::time::Duration;
use tokio_stream::{self as stream, StreamExt};
use tokio_test::assert_pending;
use tokio_test::task;

#[tokio::test(start_paused = true)]
async fn stream_chunks_remainder() -> Result<(), Box<dyn Error>> {
    let stream1 =
        stream::iter([5]).then(move |n| time::sleep(Duration::from_secs(1)).map(move |_| n));

    let inner = stream::iter([1, 2, 3, 4]).chain(stream1);
    tokio::pin!(inner);

    let chunked = (&mut inner).chunks_timeout(10, Duration::from_millis(20));

    let mut chunked = task::spawn(chunked);
    assert_pending!(chunked.poll_next());

    let remainder = chunked.enter(|_, stream| stream.into_remainder());

    assert_eq!(remainder, vec![1, 2, 3, 4]);
    time::advance(Duration::from_secs(2)).await;
    assert_eq!(inner.next().await, Some(5));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn empty_stream_does_not_allocate_large_chunk_buffer() {
    let chunked = stream::empty::<u8>().chunks_timeout(usize::MAX, Duration::from_secs(1));
    tokio::pin!(chunked);

    assert_eq!(chunked.next().await, None);
}

#[tokio::test(start_paused = true)]
async fn large_max_size_does_not_panic() {
    let chunked = stream::iter([1u8, 2, 3]).chunks_timeout(usize::MAX, Duration::from_secs(1));
    tokio::pin!(chunked);

    assert_eq!(chunked.next().await, Some(vec![1, 2, 3]));
    assert_eq!(chunked.next().await, None);
}

#[tokio::test(start_paused = true)]
async fn large_max_size_limits_initial_allocation() {
    let chunked = stream::iter([1u8, 2, 3]).chunks_timeout(4096, Duration::from_secs(1));
    tokio::pin!(chunked);

    let chunk = chunked.next().await.unwrap();
    assert!(chunk.capacity() <= 1024, "capacity: {}", chunk.capacity());
    assert_eq!(chunk, vec![1, 2, 3]);
    assert_eq!(chunked.next().await, None);
}

#[tokio::test(start_paused = true)]
async fn chunks_grow_past_initial_allocation() {
    let chunked = stream::iter(0..2051).chunks_timeout(2048, Duration::from_secs(1));
    tokio::pin!(chunked);

    assert_eq!(chunked.next().await, Some((0..2048).collect()));
    assert_eq!(chunked.next().await, Some(vec![2048, 2049, 2050]));
    assert_eq!(chunked.next().await, None);
}
