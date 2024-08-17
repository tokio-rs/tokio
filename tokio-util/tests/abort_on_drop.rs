use tokio::sync::oneshot;
use tokio_util::task::AbortOnDropHandle;

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
