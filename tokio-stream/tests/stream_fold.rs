use tokio_stream::StreamExt;

// This does not trigger coop, so we can test the combinator's coop behavior.
use futures::stream::iter;

#[cfg(feature = "rt")]
#[tokio::test]
async fn always_ready_items_are_cooperative() {
    let mut operation = tokio_test::task::spawn(iter(0..256).fold(0, |sum, n| sum + n));

    tokio_test::assert_pending!(operation.poll());
    assert_eq!(operation.await, (0..256).sum::<i32>());
}

#[tokio::test]
async fn always_ready_items_remain_unconstrained() {
    let fold = iter(vec![1; 256]).fold(0, |sum, n| sum + n);
    let mut task = tokio_test::task::spawn(tokio::task::unconstrained(fold));

    // With coop disabled, all 256 additions must finish in a single poll.
    assert_eq!(tokio_test::assert_ready!(task.poll()), 256);
}
