#![warn(rust_2018_idioms)]
#![cfg(all(feature = "time", feature = "sync", feature = "io-util"))]

use tokio::time;
use tokio_stream::{self as stream, StreamExt};

use futures::FutureExt;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn usage() {
    let iter = vec![1, 2, 3].into_iter();
    let stream0 = stream::iter(iter);

    let iter = vec![4].into_iter();
    let stream1 = stream::iter(iter)
         .then(move |n| time::sleep(Duration::from_secs(2)).map(move |_| n));

    let chunk_stream = stream0
        .chain(stream1)
        .chunks_timeout(4, Duration::from_secs(1));

    tokio::pin!(chunk_stream);

    assert_eq!(chunk_stream.next().await, Some(vec![1,2,3]));
    assert_eq!(chunk_stream.next().await, Some(vec![4]));
}
