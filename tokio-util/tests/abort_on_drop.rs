use tokio::{sync::oneshot, task::yield_now};
use tokio_util::task::{AbortOnDrop, AbortOnDropHandle};

#[tokio::test]
async fn aborts_task_on_drop() {
    let (mut tx, rx) = oneshot::channel::<bool>();
    let handle = tokio::spawn(async move {
        let _ = rx.await;
    });
    let handle = AbortOnDropHandle::new(handle);
    drop(handle);
    tx.closed().await;
    assert!(tx.is_closed());
}

#[tokio::test]
async fn aborts_task_directly() {
    let (mut tx, rx) = oneshot::channel::<bool>();
    let handle = tokio::spawn(async move {
        let _ = rx.await;
    });
    let handle = AbortOnDropHandle::new(handle);
    handle.abort();
    tx.closed().await;
    assert!(tx.is_closed());
    assert!(handle.is_finished());
}

#[tokio::test]
async fn does_not_abort_after_detach() {
    let (tx, rx) = oneshot::channel::<bool>();
    let handle = tokio::spawn(async move {
        let _ = rx.await;
    });
    let handle = AbortOnDropHandle::new(handle);
    handle.detach(); // returns and drops the original join handle
    yield_now().await;
    assert!(!tx.is_closed()); // the task is still alive
}

#[tokio::test]
async fn handle_aborts_task_on_drop() {
    let (mut tx, rx) = oneshot::channel::<bool>();
    let handle = tokio::spawn(async move {
        let _ = rx.await;
    });
    let handle = AbortOnDrop::new(handle.abort_handle());
    drop(handle);
    tx.closed().await;
    assert!(tx.is_closed());
}

#[tokio::test]
async fn handle_aborts_task_directly() {
    let (mut tx, rx) = oneshot::channel::<bool>();
    let handle = tokio::spawn(async move {
        let _ = rx.await;
    });
    let handle = AbortOnDrop::new(handle.abort_handle());
    handle.abort();
    tx.closed().await;
    assert!(tx.is_closed());
    assert!(handle.is_finished());
}

#[tokio::test]
async fn handle_does_not_abort_after_detach() {
    let (tx, rx) = oneshot::channel::<bool>();
    let handle = tokio::spawn(async move {
        let _ = rx.await;
    });
    let handle = AbortOnDrop::new(handle.abort_handle());
    handle.detach(); // returns and drops the original abort handle
    yield_now().await;
    assert!(!tx.is_closed()); // the task is still alive
}
