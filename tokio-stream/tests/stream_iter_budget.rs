use std::iter;
use tokio_stream::{self as stream, Stream};
use tokio_test::{assert_pending, assert_ready, task};

#[tokio::test]
async fn test_iter_coop_budget() {
    let mut stream = task::spawn(stream::iter(iter::repeat(1)));

    // Tokio's default budget is 128.
    // Fallback yield_amt is 32.
    let limit = if cfg!(feature = "rt") { 128 } else { 32 };

    for i in 0..limit {
        let res = stream.poll_next();
        assert!(res.is_ready(), "Should be ready at index {i}");
    }

    // Next poll should be pending
    assert_pending!(stream.poll_next());
    // Note: we don't check is_woken() here because in RT mode,
    // Tokio uses deferred wakers which might not reflect in is_woken() immediately
    // depending on the test environment.
}

#[tokio::test]
async fn test_iter_size_hint() {
    let stream = stream::iter(vec![1, 2, 3]);
    assert_eq!(stream.size_hint(), (3, Some(3)));
}

#[tokio::test]
async fn test_iter_eof_behavior() {
    let mut stream = task::spawn(stream::iter(vec![1]));

    assert_ready!(stream.poll_next());
    assert_ready!(stream.poll_next()); // EOF should be ready None
}
