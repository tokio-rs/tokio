#![warn(rust_2018_idioms)]

use std::rc::Rc;
use tokio_util::task::spawn_pinned;

#[tokio::test]
#[ignore]
async fn can_spawn_not_send_futures() {
    let closure = move || {
        // Rc is Send + !Sync
        let local_data = Rc::new("test");
        async move { local_data.to_string() }
    };
    let output = spawn_pinned(closure).await.unwrap();

    assert_eq!(output, "test");
    println!("At end");
}
