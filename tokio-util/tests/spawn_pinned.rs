#![warn(rust_2018_idioms)]

use futures::{FutureExt, TryFutureExt};
use std::rc::Rc;
use tokio_util::task;

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

#[test]
#[should_panic(expected = "assertion failed: pool_size > 0")]
fn cannot_create_zero_sized_pool() {
    let _pool = task::new_local_pool(0);
}

#[tokio::test]
async fn can_spawn_multiple_futures() {
    let pool = task::LocalPoolHandle::new(2);

    let future1 = pool
        .spawn_pinned(|| {
            let local_data = Rc::new("test1");
            async move { local_data.to_string() }
        })
        .unwrap_or_else(|e| panic!("Join error: {}", e));
    let future2 = pool
        .spawn_pinned(|| {
            let local_data = Rc::new("test2");
            async move { local_data.to_string() }
        })
        .unwrap_or_else(|e| panic!("Join error: {}", e));

    assert_eq!(future1.await, "test1");
    assert_eq!(future2.await, "test2");
}
