#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::thread::sleep;
use std::time::Duration;

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

/// Checks that a suspended LocalSet task can be aborted from a remote thread
/// without panicking and without running the tasks destructor on the wrong thread.
/// <https://github.com/tokio-rs/tokio/issues/3929>
#[test]
fn remote_abort_local_set_3929() {
    struct DropCheck {
        created_on: std::thread::ThreadId,
        not_send: std::marker::PhantomData<*const ()>,
    }

    impl DropCheck {
        fn new() -> Self {
            Self {
                created_on: std::thread::current().id(),
                not_send: std::marker::PhantomData,
            }
        }
    }
    impl Drop for DropCheck {
        fn drop(&mut self) {
            if std::thread::current().id() != self.created_on {
                panic!("non-Send value dropped in another thread!");
            }
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();

    let check = DropCheck::new();
    let jh = local.spawn_local(async move {
        futures::future::pending::<()>().await;
        drop(check);
    });

    let jh2 = std::thread::spawn(move || {
        sleep(Duration::from_millis(50));
        jh.abort();
    });

    rt.block_on(local);
    jh2.join().unwrap();
}
