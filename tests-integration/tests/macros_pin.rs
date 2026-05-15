use futures::executor::block_on;

async fn my_async_fn() {}

#[test]
fn pin() {
    block_on(async {
        let future = my_async_fn();
        tokio::pin!(future);
        (&mut future).await
    });
}
