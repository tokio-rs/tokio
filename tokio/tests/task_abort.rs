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

/// Checks that a suspended task can be aborted inside of a current_thread
/// executor without panicking as reported in issue #3662:
/// <https://github.com/tokio-rs/tokio/issues/3662>.
#[test]
fn test_abort_without_panic_3662() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct DropCheck(Arc<AtomicBool>);

    impl Drop for DropCheck {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async move {
        let drop_flag = Arc::new(AtomicBool::new(false));
        let drop_check = DropCheck(drop_flag.clone());

        let j = tokio::spawn(async move {
            // NB: just grab the drop check here so that it becomes part of the
            // task.
            let _drop_check = drop_check;
            futures::future::pending::<()>().await;
        });

        let drop_flag2 = drop_flag.clone();

        let task = std::thread::spawn(move || {
            // This runs in a separate thread so it doesn't have immediate
            // thread-local access to the executor. It does however transition
            // the underlying task to be completed, which will cause it to be
            // dropped (in this thread no less).
            assert!(!drop_flag2.load(Ordering::SeqCst));
            j.abort();
            // TODO: is this guaranteed at this point?
            // assert!(drop_flag2.load(Ordering::SeqCst));
            j
        })
        .join()
        .unwrap();

        assert!(drop_flag.load(Ordering::SeqCst));
        let result = task.await;
        assert!(result.unwrap_err().is_cancelled());

        // Note: We do the following to trigger a deferred task cleanup.
        //
        // The relevant piece of code you want to look at is in:
        // `Inner::block_on` of `basic_scheduler.rs`.
        //
        // We cause the cleanup to happen by having a poll return Pending once
        // so that the scheduler can go into the "auxiliary tasks" mode, at
        // which point the task is removed from the scheduler.
        let i = tokio::spawn(async move {
            tokio::task::yield_now().await;
        });

        i.await.unwrap();
    });
}
