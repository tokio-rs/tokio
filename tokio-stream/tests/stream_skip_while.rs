#![cfg(feature = "rt")]

use tokio_stream::StreamExt;

// This does not trigger coop, so we can test the combinator's coop behavior.
use futures::stream::iter;

#[tokio::test]
async fn skipped_items_are_cooperative() {
    let mut stream = iter(0..256).skip_while(|n| *n < 255);
    let mut next = tokio_test::task::spawn(stream.next());

    tokio_test::assert_pending!(next.poll());
    assert_eq!(next.await, Some(255));
    assert_eq!(stream.next().await, None);
}
