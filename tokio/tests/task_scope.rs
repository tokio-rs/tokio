#![warn(rust_2018_idioms)]
#![cfg(tokio_unstable)]
#![cfg(feature = "full")]

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{select, task::scope, time::delay_for};

#[derive(Clone)]
struct AtomicFlag(Arc<AtomicBool>);

impl AtomicFlag {
    fn new() -> Self {
        AtomicFlag(Arc::new(AtomicBool::new(false)))
    }

    fn is_set(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn set(&self) {
        self.0.store(true, Ordering::Release);
    }
}

struct SetFlagOnDropGuard {
    flag: AtomicFlag,
}

impl Drop for SetFlagOnDropGuard {
    fn drop(&mut self) {
        self.flag.set();
    }
}

#[tokio::test]
async fn unused_scope() {
    let scope = scope::enter(async {});
    drop(scope);
}

#[tokio::test]
async fn spawn_and_return_result() {
    let result = scope::enter(async move {
        let handle = scope::spawn(async {
            tokio::time::delay_for(std::time::Duration::from_millis(500)).await;
            123u32
        });
        handle.await
    })
    .await;
    assert_eq!(123u32, result.unwrap());
}

#[tokio::test]
async fn cancel_and_wait_for_child_task() {
    let flag = AtomicFlag::new();
    let flag_clone = flag.clone();

    let result = scope::enter(async move {
        let handle = scope::spawn(async {
            delay_for(Duration::from_millis(20)).await;
            123u32
        });

        scope::spawn_cancellable(async {
            let _guard = SetFlagOnDropGuard { flag: flag_clone };
            loop {
                tokio::task::yield_now().await;
            }
        });

        handle.await
    })
    .await;
    assert_eq!(123u32, result.unwrap());

    // Check that the second task was cancelled
    assert_eq!(true, flag.is_set());
}

#[tokio::test]
async fn graceful_cancellation() {
    let result = scope::enter(async move {
        scope::spawn(async {
            delay_for(Duration::from_millis(20)).await;
            scope::current_cancellation_token().cancel();
            123u32
        });

        scope::spawn(async {
            let ct = scope::current_cancellation_token();
            select! {
                _ = ct.cancelled() => {
                    1
                },
                _ = delay_for(Duration::from_millis(5000)) => {
                    2
                },
            }
        })
        .await
    })
    .await;
    assert_eq!(1, result.unwrap());
}

#[tokio::test]
async fn cancels_nested_scopes() {
    let flag = AtomicFlag::new();
    let flag_clone = flag.clone();

    let result = scope::enter(async move {
        let ct = scope::current_cancellation_token();

        let handle = scope::spawn(async move {
            delay_for(Duration::from_millis(200)).await;
            // Cancelling the parent scope should also cancel the task
            // which is running insie a child scope
            ct.cancel();
            123u32
        });

        scope::enter(async move {
            dbg!("Start of scope");
            let _ = scope::spawn_cancellable(async {
                let _guard = SetFlagOnDropGuard { flag: flag_clone };
                loop {
                    tokio::task::yield_now().await;
                }
            })
            .await;
        })
        .await;

        handle.await
    })
    .await;
    assert_eq!(123u32, result.unwrap());

    // Check that the second task was cancelled
    assert_eq!(true, flag.is_set());
}

#[tokio::test]
async fn wait_until_non_joined_tasks_complete() {
    let flag = AtomicFlag::new();
    let flag_clone = flag.clone();
    let start_time = Instant::now();

    let _ = scope::enter(async move {
        let handle = scope::spawn(async {
            delay_for(Duration::from_millis(20)).await;
            123u32
        });

        scope::spawn(async move {
            tokio::time::delay_for(Duration::from_millis(100)).await;
            flag_clone.set();
        });

        handle.await
    })
    .await;

    assert!(start_time.elapsed() >= Duration::from_millis(100));

    // Check that the second task run to completion
    assert_eq!(true, flag.is_set());
}

#[should_panic]
#[tokio::test]
async fn panic_if_active_scope_is_dropped() {
    let scope_fut = scope::enter(async move {
        let handle = scope::spawn(async {
            delay_for(Duration::from_millis(20)).await;
            123u32
        });

        // Spawn a long running task which prevents the task from finishing
        scope::spawn(async move {
            tokio::time::delay_for(Duration::from_millis(1000)).await;
        });

        handle.await
    });

    select! {
        _ = scope_fut => {
            panic!("Scope should not complete");
        },
        _ = delay_for(Duration::from_millis(50)) => {
            // Drop the scope here
        },
    };
}
