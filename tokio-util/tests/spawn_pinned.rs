#![warn(rust_2018_idioms)]

use std::rc::Rc;
use tokio_util::task;

#[tokio::test]
async fn can_spawn_not_send_future() {
    let pool = task::new_local_pool(1);

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
