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
