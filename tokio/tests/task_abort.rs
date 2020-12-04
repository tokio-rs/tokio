#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

/// Checks that a suspended task can be aborted without panicking as reported in
/// issue #3157: <https://github.com/tokio-rs/tokio/issues/3157>.
#[test]
fn test_abort_without_panic_3157() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .worker_threads(1)
        .build()
        .unwrap();

    rt.block_on(async move {
        let handle = tokio::spawn(async move {
            println!("task started");
            tokio::time::sleep(std::time::Duration::new(100, 0)).await
        });

        // wait for task to sleep.
        tokio::time::sleep(std::time::Duration::new(1, 0)).await;

        handle.abort();
        let _ = handle.await;
    });
}
