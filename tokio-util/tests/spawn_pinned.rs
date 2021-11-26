#![warn(rust_2018_idioms)]

use std::rc::Rc;
use tokio_util::task;

// Simple test of running a !Send future via spawn_pinned
#[tokio::test]
async fn can_spawn_not_send_future() {
    let pool = task::LocalPoolHandle::new(1);

    let output = pool
        .spawn_pinned(|| {
            // Rc is !Send + !Sync
            let local_data = Rc::new("test");

            // This future holds an Rc, so it is !Send
            async move { local_data.to_string() }
        })
        .await
        .unwrap();

    assert_eq!(output, "test");
}

// Dropping the join handle still lets the task execute
#[test]
fn can_drop_future_and_still_get_output() {
    let pool = task::LocalPoolHandle::new(1);
    let (sender, receiver) = std::sync::mpsc::channel();

    let _ = pool.spawn_pinned(move || {
        // Rc is !Send + !Sync
        let local_data = Rc::new("test");

        // This future holds an Rc, so it is !Send
        async move {
            let _ = sender.send(local_data.to_string());
        }
    });

    assert_eq!(receiver.recv(), Ok("test".to_string()));
}

#[test]
#[should_panic(expected = "assertion failed: pool_size > 0")]
fn cannot_create_zero_sized_pool() {
    let _pool = task::LocalPoolHandle::new(0);
}

// We should be able to spawn multiple futures onto the pool at the same time.
#[tokio::test]
async fn can_spawn_multiple_futures() {
    let pool = task::LocalPoolHandle::new(2);

    let join_handle1 = pool.spawn_pinned(|| {
        let local_data = Rc::new("test1");
        async move { local_data.to_string() }
    });
    let join_handle2 = pool.spawn_pinned(|| {
        let local_data = Rc::new("test2");
        async move { local_data.to_string() }
    });

    assert_eq!(join_handle1.await.unwrap(), "test1");
    assert_eq!(join_handle2.await.unwrap(), "test2");
}

// A panic in the spawned task causes the join handle to return an error.
// But, you can continue to spawn tasks.
#[tokio::test]
async fn task_panic_propagates() {
    let pool = task::LocalPoolHandle::new(1);

    let join_handle = pool.spawn_pinned(|| async {
        panic!("Test panic");
    });

    let result = join_handle.await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.is_panic());

    // Trying again with a "safe" task still works
    let join_handle = pool.spawn_pinned(|| async { "test" });
    let result = join_handle.await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test");
}

// A panic in the task creation callback kills the worker.
// See TODOs in spawn_pinned implementation for possible fixes.
#[tokio::test]
async fn callback_panic_kills_worker() {
    let pool = task::LocalPoolHandle::new(1);

    let join_handle = pool.spawn_pinned(|| {
        panic!("Test panic");
        #[allow(unreachable_code)]
        async {}
    });

    let result = join_handle.await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.is_cancelled());

    // Trying again with a "safe" callback still fails
    let join_handle = pool.spawn_pinned(|| async { "test" });
    let result = join_handle.await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.is_cancelled());
}
