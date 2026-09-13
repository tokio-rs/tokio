use tokio_stream::StreamExt;

// This does not trigger coop, so we can test the combinator's coop behavior.
use futures::stream::iter;

#[tokio::test]
async fn nested_combinators_remain_unconstrained() {
    let expected = vec![42; 256];
    // Every combinator passes all items through unchanged.
    let collect = iter(expected.clone())
        .filter(|_| true) // Keep every item.
        .filter_map(Some) // Keep every item unchanged.
        .skip(0) // Skip no items.
        .skip_while(|_| false) // Skip no items.
        .collect::<Vec<_>>(); // Collect all items into a vector.
    let mut task = tokio_test::task::spawn(tokio::task::unconstrained(collect));

    // With coop disabled, all stages must finish in a single poll.
    let values = tokio_test::assert_ready!(task.poll());
    assert_eq!(values, expected);
}

#[cfg(feature = "rt")]
#[tokio::test]
async fn nested_combinators_are_cooperative() {
    let expected = vec![42; 256];
    // Every combinator passes all items through unchanged.
    let collect = iter(expected.clone())
        .filter(|_| true) // Keep every item.
        .filter_map(Some) // Keep every item unchanged.
        .skip(0) // Skip no items.
        .skip_while(|_| false) // Skip no items.
        .collect::<Vec<_>>(); // Collect all items into a vector.
    let mut task = tokio_test::task::spawn(collect);

    // With coop enabled, processing these ready items must yield.
    tokio_test::assert_pending!(task.poll());
    assert_eq!(task.await, expected);
}

#[tokio::test]
async fn deeply_nested_combinators_reach_the_source() {
    let mut stream: Box<dyn tokio_stream::Stream<Item = u8> + Unpin> = Box::new(iter([42]));
    for _ in 0..256 {
        stream = Box::new(
            stream
                .filter(|_| true) // Keep every item.
                .filter_map(Some) // Keep every item unchanged.
                .skip(0) // Skip no items.
                .skip_while(|_| false), // Skip no items.,
        );
    }
    let mut next = tokio_test::task::spawn(stream.next());

    // Passing an item through must not spend the budget before reaching the source.
    assert_eq!(tokio_test::assert_ready!(next.poll()), Some(42));
}
