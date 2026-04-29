use futures_core::FusedStream;
use tokio_stream::StreamExt;

// Helper: a fused base stream built from a vec
fn fused_iter<T>(items: Vec<T>) -> impl FusedStream<Item = T> {
    tokio_stream::iter(items).fuse()
}

// ── map ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn map_not_terminated_before_done() {
    let stream = fused_iter(vec![1, 2]).map(|x| x * 2);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn map_terminated_after_inner_done() {
    let mut stream = fused_iter(vec![1]).map(|x| x * 2);
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

// ── filter ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn filter_not_terminated_before_done() {
    let stream = fused_iter(vec![1, 2]).filter(|x| *x > 0);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn filter_terminated_after_inner_done() {
    let mut stream = fused_iter(vec![1]).filter(|x| *x > 0);
    assert_eq!(stream.next().await, Some(1));
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

// ── filter_map ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn filter_map_not_terminated_before_done() {
    let stream = fused_iter(vec![1, 2]).filter_map(|x| Some(x));
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn filter_map_terminated_after_inner_done() {
    let mut stream = fused_iter(vec![1]).filter_map(|x| Some(x * 10));
    assert_eq!(stream.next().await, Some(10));
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

// ── skip ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn skip_not_terminated_before_done() {
    let stream = fused_iter(vec![1, 2, 3]).skip(1);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn skip_terminated_after_inner_done() {
    let mut stream = fused_iter(vec![1, 2]).skip(1);
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

// ── skip_while ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn skip_while_not_terminated_before_done() {
    let stream = fused_iter(vec![1, 2, 3]).skip_while(|x| *x < 2);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn skip_while_terminated_after_inner_done() {
    let mut stream = fused_iter(vec![1, 2]).skip_while(|x| *x < 2);
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

// ── take ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn take_not_terminated_before_limit() {
    let stream = tokio_stream::iter(vec![1, 2, 3]).take(2);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn take_terminated_when_remaining_zero() {
    let mut stream = tokio_stream::iter(vec![1, 2]).take(2);
    assert_eq!(stream.next().await, Some(1));
    assert!(!stream.is_terminated());
    assert_eq!(stream.next().await, Some(2));
    // remaining hits 0 after getting the second item
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

#[tokio::test]
async fn take_zero_is_immediately_terminated() {
    let stream = tokio_stream::iter(vec![1, 2]).take(0);
    assert!(stream.is_terminated());
}

// ── take_while ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn take_while_not_terminated_before_predicate_fails() {
    let stream = tokio_stream::iter(vec![1, 2, 3]).take_while(|x| *x < 10);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn take_while_terminated_after_predicate_fails() {
    let mut stream = tokio_stream::iter(vec![1, 5, 2]).take_while(|x| *x < 3);
    assert_eq!(stream.next().await, Some(1));
    assert!(!stream.is_terminated());
    // predicate fails on 5 → done flag set
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
}

// ── then ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn then_not_terminated_before_done() {
    let stream = fused_iter(vec![1, 2]).then(|x| async move { x * 2 });
    tokio::pin!(stream);
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn then_terminated_after_inner_done_and_no_pending_future() {
    let stream = fused_iter(vec![1]).then(|x| async move { x * 2 });
    tokio::pin!(stream);
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.next().await, None);
    // inner stream done AND no in-flight future
    assert!(stream.is_terminated());
}

// ── chain ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn chain_not_terminated_while_either_has_items() {
    let stream = fused_iter(vec![1]).chain(fused_iter(vec![2]));
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn chain_terminated_only_after_both_done() {
    let mut stream = fused_iter(vec![1]).chain(fused_iter(vec![2]));
    assert_eq!(stream.next().await, Some(1));
    assert!(!stream.is_terminated()); // b still has items
    assert_eq!(stream.next().await, Some(2));
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated()); // both done now
}

// ── merge ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn merge_not_terminated_while_either_has_items() {
    let stream = fused_iter(vec![1]).merge(fused_iter(vec![2]));
    assert!(!stream.is_terminated());
}

#[tokio::test]
async fn merge_terminated_only_after_both_done() {
    let mut stream = fused_iter(vec![1]).merge(fused_iter(vec![2]));
    // drain both
    let mut collected = vec![];
    while let Some(x) = stream.next().await {
        collected.push(x);
    }
    assert_eq!(stream.next().await, None);
    assert!(stream.is_terminated());
    assert_eq!(collected.len(), 2);
}
