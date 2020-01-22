#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use futures::{select, FutureExt};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    task::{
        scope, scope_with_options, ScopeCancelBehavior, ScopeDropBehavior, ScopeHandle,
        ScopeOptions, ScopedJoinHandle,
    },
    time::delay_for,
};

#[tokio::test]
async fn unused_scope() {
    let scope = scope(|_scope| async {});
    drop(scope);
}

#[tokio::test]
async fn spawn_and_return_result() {
    let result = scope(|scope| {
        async move {
            let handle = scope.spawn(async { 123u32 });
            handle.await
        }
    })
    .await;
    assert_eq!(123u32, result.unwrap());
}

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
async fn cancel_and_wait_for_child_task() {
    let flag = AtomicFlag::new();
    let flag_clone = flag.clone();

    let result = scope(|scope| {
        async move {
            let handle = scope.spawn(async {
                delay_for(Duration::from_millis(20)).await;
                123u32
            });

            scope.spawn(async {
                let _guard = SetFlagOnDropGuard { flag: flag_clone };
                loop {
                    tokio::task::yield_now().await;
                }
            });

            handle.await
        }
    })
    .await;
    assert_eq!(123u32, result.unwrap());

    // Check that the second task was cancelled
    assert_eq!(true, flag.is_set());
}

#[tokio::test]
async fn run_task_to_completion_if_configured() {
    let flag = AtomicFlag::new();
    let flag_clone = flag.clone();

    let options = ScopeOptions {
        drop_behavior: ScopeDropBehavior::Panic,
        cancel_behavior: ScopeCancelBehavior::ContinueChildTasks,
    };

    let result = scope_with_options(options, |scope| {
        async move {
            let handle = scope.spawn(async {
                delay_for(Duration::from_millis(20)).await;
                123u32
            });

            scope.spawn(async move {
                // This should run to completion - even if it takes longer
                delay_for(Duration::from_millis(50)).await;
                flag_clone.set();
            });

            handle.await
        }
    })
    .await;
    assert_eq!(123u32, result.unwrap());

    // Check that the second task run to completion
    assert_eq!(true, flag.is_set());
}

#[test]
fn block_until_non_joined_tasks_complete() {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let flag = AtomicFlag::new();
        let flag_clone = flag.clone();

        let start_time = Instant::now();
        let scope_fut = scope(|scope| {
            async move {
                let handle = scope.spawn(async {
                    delay_for(Duration::from_millis(20)).await;
                    123u32
                });

                scope.spawn(async move {
                    // Use block_in_place makes the task not cancellable
                    tokio::task::block_in_place(|| {
                        std::thread::sleep(Duration::from_millis(100));
                    });
                    flag_clone.set();
                });

                handle.await
            }
        });

        select! {
            _ = scope_fut.fuse() => {
                panic!("Scope should not complete");
            },
            _ = delay_for(Duration::from_millis(50)).fuse() => {
                // Drop the scope here
            },
        };

        assert!(start_time.elapsed() >= Duration::from_millis(100));

        // Check that the second task run to completion
        assert_eq!(true, flag.is_set());
    });
}

#[should_panic]
#[test]
fn panic_if_active_scope_is_dropped() {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut options = ScopeOptions::default();
        options.drop_behavior = ScopeDropBehavior::Panic;

        let scope_fut = scope_with_options(options, |scope| {
            async move {
                let handle = scope.spawn(async {
                    delay_for(Duration::from_millis(20)).await;
                    123u32
                });

                scope.spawn(async move {
                    // Use block_in_place makes the task not cancellable
                    tokio::task::block_in_place(|| {
                        std::thread::sleep(Duration::from_millis(100));
                    });
                });

                handle.await
            }
        });

        select! {
            _ = scope_fut.fuse() => {
                panic!("Scope should not complete");
            },
            _ = delay_for(Duration::from_millis(50)).fuse() => {
                // Drop the scope here
            },
        };
    });
}

#[test]
fn child_tasks_can_continue_to_run_if_configured() {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let flag = AtomicFlag::new();
        let flag_clone = flag.clone();

        let mut options = ScopeOptions::default();
        options.drop_behavior = ScopeDropBehavior::ContinueTasks;

        let start_time = Instant::now();
        let scope_fut = scope_with_options(options, |scope| {
            async move {
                let handle = scope.spawn(async {
                    delay_for(Duration::from_millis(20)).await;
                    123u32
                });

                scope.spawn(async move {
                    // Use block_in_place makes the task not cancellable
                    tokio::task::block_in_place(|| {
                        std::thread::sleep(Duration::from_millis(100));
                    });
                    flag_clone.set();
                });

                handle.await
            }
        });

        select! {
            _ = scope_fut.fuse() => {
                panic!("Scope should not complete");
            },
            _ = delay_for(Duration::from_millis(50)).fuse() => {
                // Drop the scope here
            },
        };

        let elapsed = start_time.elapsed();
        assert!(elapsed >= Duration::from_millis(50) && elapsed < Duration::from_millis(100));
        assert_eq!(false, flag.is_set());

        // Wait until the leaked task run to completion
        delay_for(Duration::from_millis(60)).await;
        assert_eq!(true, flag.is_set());
    });
}

#[test]
fn clone_scope_handles_and_cancel_child() {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let drop_flag = AtomicFlag::new();
        let drop_flag_clone = drop_flag.clone();
        let completion_flag = AtomicFlag::new();
        let completion_flag_clone = completion_flag.clone();

        scope(|scope| {
            async move {
                let cloned_handle = scope.clone();

                let join_handle = scope.spawn(async move {
                    delay_for(Duration::from_millis(20)).await;
                    // Spawn another task - which is not awaited
                    let _join_handle = cloned_handle.spawn(async move {
                        let _guard = SetFlagOnDropGuard {
                            flag: drop_flag_clone,
                        };

                        delay_for(Duration::from_millis(50)).await;
                        // This should not get executed, since the inital task exits before
                        // and this task gets cancelled.
                        completion_flag_clone.set();
                    });
                });

                let _ = join_handle.await;
            }
        })
        .await;

        assert_eq!(true, drop_flag.is_set());
        assert_eq!(false, completion_flag.is_set());
    });
}

#[test]
fn clone_scope_handles_and_wait_for_child() {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let drop_flag = AtomicFlag::new();
        let drop_flag_clone = drop_flag.clone();
        let completion_flag = AtomicFlag::new();
        let completion_flag_clone = completion_flag.clone();

        let mut options = ScopeOptions::default();
        options.cancel_behavior = ScopeCancelBehavior::ContinueChildTasks;

        let start_time = Instant::now();
        scope_with_options(options, |scope| {
            async move {
                let cloned_handle = scope.clone();

                let join_handle = scope.spawn(async move {
                    delay_for(Duration::from_millis(20)).await;
                    // Spawn another task - which is not awaited
                    let _join_handle = cloned_handle.spawn(async move {
                        let _guard = SetFlagOnDropGuard {
                            flag: drop_flag_clone,
                        };

                        delay_for(Duration::from_millis(50)).await;
                        // This should get executed, since tasks are allowed to run
                        // to completion.
                        completion_flag_clone.set();
                    });
                });

                let _ = join_handle.await;
            }
        })
        .await;

        assert!(start_time.elapsed() >= Duration::from_millis(70));

        assert_eq!(true, drop_flag.is_set());
        assert_eq!(true, completion_flag.is_set());
    });
}
