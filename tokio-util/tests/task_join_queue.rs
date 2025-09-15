#![warn(rust_2018_idioms)]

use tokio::sync::oneshot;
use tokio::task::yield_now;
use tokio_test::{assert_pending, assert_ready, task};
use tokio_util::task::JoinQueue;

#[tokio::test]
async fn test_no_spurious_wakeups() {
    // 1. Create a JoinQueue with a task on it. The task can wait on an oneshot channel so you can control it.
    let (tx, rx) = oneshot::channel::<()>();
    let mut join_queue = JoinQueue::new();
    join_queue.spawn(async move {
        let _ = rx.await;
        42
    });

    let mut join_next = task::spawn(join_queue.join_next());

    assert_pending!(join_next.poll());

    assert!(!join_next.is_woken());

    let _ = tx.send(());
    yield_now().await;

    assert!(join_next.is_woken());

    let output = assert_ready!(join_next.poll());
    assert_eq!(output.unwrap().unwrap(), 42);
}
