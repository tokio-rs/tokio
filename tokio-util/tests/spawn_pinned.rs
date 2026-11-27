#![warn(rust_2018_idioms)]
#![cfg(not(target_os = "wasi"))] // Wasi doesn't support threads

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Barrier;
use tokio_util::task;

/// Simple test of running a !Send future via spawn_pinned
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

/// Dropping the join handle still lets the task execute
#[test]
fn can_drop_future_and_still_get_output() {
    let pool = task::LocalPoolHandle::new(1);
    let (sender, receiver) = std::sync::mpsc::channel();

    pool.spawn_pinned(move || {
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

/// We should be able to spawn multiple futures onto the pool at the same time.
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

/// A panic in the spawned task causes the join handle to return an error.
/// But, you can continue to spawn tasks.
#[tokio::test]
#[cfg(panic = "unwind")]
async fn task_panic_propagates() {
    let pool = task::LocalPoolHandle::new(1);

    let join_handle = pool.spawn_pinned(|| async {
        panic!("Test panic");
    });

    let result = join_handle.await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.is_panic());
    let panic_str = error.into_panic().downcast::<&'static str>().unwrap();
    assert_eq!(*panic_str, "Test panic");

    // Trying again with a "safe" task still works
    let join_handle = pool.spawn_pinned(|| async { "test" });
    let result = join_handle.await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test");
}

/// A panic during task creation causes the join handle to return an error.
/// But, you can continue to spawn tasks.
#[tokio::test]
#[cfg(panic = "unwind")]
async fn callback_panic_does_not_kill_worker() {
    let pool = task::LocalPoolHandle::new(1);

    let join_handle = pool.spawn_pinned(|| {
        panic!("Test panic");
        #[allow(unreachable_code)]
        async {}
    });

    let result = join_handle.await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.is_panic());
    let panic_str = error.into_panic().downcast::<&'static str>().unwrap();
    assert_eq!(*panic_str, "Test panic");

    // Trying again with a "safe" callback works
    let join_handle = pool.spawn_pinned(|| async { "test" });
    let result = join_handle.await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test");
}

/// Canceling the task via the returned join handle cancels the spawned task
/// (which has a different, internal join handle).
#[tokio::test]
async fn task_cancellation_propagates() {
    let pool = task::LocalPoolHandle::new(1);
    let notify_dropped = Arc::new(());
    let weak_notify_dropped = Arc::downgrade(&notify_dropped);

    let (start_sender, start_receiver) = tokio::sync::oneshot::channel();
    let (drop_sender, drop_receiver) = tokio::sync::oneshot::channel::<()>();
    let join_handle = pool.spawn_pinned(|| async move {
        let _drop_sender = drop_sender;
        // Move the Arc into the task
        let _notify_dropped = notify_dropped;
        let _ = start_sender.send(());

        // Keep the task running until it gets aborted
        futures::future::pending::<()>().await;
    });

    // Wait for the task to start
    let _ = start_receiver.await;

    join_handle.abort();

    // Wait for the inner task to abort, dropping the sender.
    // The top level join handle aborts quicker than the inner task (the abort
    // needs to propagate and get processed on the worker thread), so we can't
    // just await the top level join handle.
    let _ = drop_receiver.await;

    // Check that the Arc has been dropped. This verifies that the inner task
    // was canceled as well.
    assert!(weak_notify_dropped.upgrade().is_none());
}

/// Tasks should be given to the least burdened worker. When spawning two tasks
/// on a pool with two empty workers the tasks should be spawned on separate
/// workers.
#[tokio::test]
async fn tasks_are_balanced() {
    let pool = task::LocalPoolHandle::new(2);

    // Spawn a task so one thread has a task count of 1
    let (start_sender1, start_receiver1) = tokio::sync::oneshot::channel();
    let (end_sender1, end_receiver1) = tokio::sync::oneshot::channel();
    let join_handle1 = pool.spawn_pinned(|| async move {
        let _ = start_sender1.send(());
        let _ = end_receiver1.await;
        std::thread::current().id()
    });

    // Wait for the first task to start up
    let _ = start_receiver1.await;

    // This task should be spawned on the other thread
    let (start_sender2, start_receiver2) = tokio::sync::oneshot::channel();
    let join_handle2 = pool.spawn_pinned(|| async move {
        let _ = start_sender2.send(());
        std::thread::current().id()
    });

    // Wait for the second task to start up
    let _ = start_receiver2.await;

    // Allow the first task to end
    let _ = end_sender1.send(());

    let thread_id1 = join_handle1.await.unwrap();
    let thread_id2 = join_handle2.await.unwrap();

    // Since the first task was active when the second task spawned, they should
    // be on separate workers/threads.
    assert_ne!(thread_id1, thread_id2);
}

#[tokio::test]
async fn spawn_by_idx() {
    let pool = task::LocalPoolHandle::new(3);
    let barrier = Arc::new(Barrier::new(4));
    let barrier1 = barrier.clone();
    let barrier2 = barrier.clone();
    let barrier3 = barrier.clone();

    let handle1 = pool.spawn_pinned_by_idx(
        || async move {
            barrier1.wait().await;
            std::thread::current().id()
        },
        0,
    );
    pool.spawn_pinned_by_idx(
        || async move {
            barrier2.wait().await;
            std::thread::current().id()
        },
        0,
    );
    let handle2 = pool.spawn_pinned_by_idx(
        || async move {
            barrier3.wait().await;
            std::thread::current().id()
        },
        1,
    );

    let loads = pool.get_task_loads_for_each_worker();
    barrier.wait().await;
    assert_eq!(loads[0], 2);
    assert_eq!(loads[1], 1);
    assert_eq!(loads[2], 0);

    let thread_id1 = handle1.await.unwrap();
    let thread_id2 = handle2.await.unwrap();

    assert_ne!(thread_id1, thread_id2);
}

/// Aborting a pinned task before the worker picks it up skips creating the
/// future, returns a cancelled `JoinError`, and decrements the worker load.
#[tokio::test]
async fn abort_before_worker_polls_task() {
    let pool = task::LocalPoolHandle::new(1);
    let entered = Arc::new(std::sync::Barrier::new(2));
    let release = Arc::new(std::sync::Barrier::new(2));

    let entered_clone = Arc::clone(&entered);
    let release_clone = Arc::clone(&release);
    let blocker = pool.spawn_pinned(move || async move {
        entered_clone.wait();
        release_clone.wait();
    });

    // Wait until the single worker thread is occupied inside `blocker`.
    entered.wait();

    let created = Arc::new(AtomicBool::new(false));
    let created_clone = Arc::clone(&created);
    let handle = pool.spawn_pinned(move || {
        created_clone.store(true, Ordering::SeqCst);
        async {}
    });

    handle.abort();
    release.wait();

    let err = handle.await.unwrap_err();
    assert!(err.is_cancelled());
    assert!(!created.load(Ordering::SeqCst));

    blocker.await.unwrap();
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);
}

/// Aborting a running `!Send` task drops the future on the pinned worker thread
/// before `JoinHandle::await` completes and decrements the worker load.
#[tokio::test]
async fn abort_running_task_drops_on_worker_thread() {
    struct ThreadCheckDrop {
        worker_tid: std::thread::ThreadId,
        dropped: Arc<AtomicBool>,
        _not_send: Rc<()>,
    }

    impl Drop for ThreadCheckDrop {
        fn drop(&mut self) {
            assert_eq!(std::thread::current().id(), self.worker_tid);
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    let pool = task::LocalPoolHandle::new(1);
    let dropped = Arc::new(AtomicBool::new(false));
    let dropped_clone = Arc::clone(&dropped);
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();

    let handle = pool.spawn_pinned(move || {
        let guard = ThreadCheckDrop {
            worker_tid: std::thread::current().id(),
            dropped: dropped_clone,
            _not_send: Rc::new(()),
        };
        async move {
            let _guard = guard;
            let _ = started_tx.send(());
            futures::future::pending::<()>().await;
        }
    });

    started_rx.await.unwrap();
    handle.abort();

    let err = handle.await.unwrap_err();
    assert!(err.is_cancelled());
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);
}

/// `tokio::task::spawn_local` works both inside the task creation callback and
/// inside the returned `!Send` future, executing on the same pinned thread.
#[tokio::test]
async fn can_spawn_local_inside_callback_and_future() {
    let pool = task::LocalPoolHandle::new(2);

    let (parent_tid, cb_tid, fut_tid) = pool
        .spawn_pinned(|| {
            let local_cb = Rc::new(10usize);
            let cb_handle =
                tokio::task::spawn_local(async move { (*local_cb, std::thread::current().id()) });

            let local_fut = Rc::new(20usize);
            async move {
                let fut_handle =
                    tokio::task::spawn_local(
                        async move { (*local_fut, std::thread::current().id()) },
                    );

                let (cb_val, cb_tid) = cb_handle.await.unwrap();
                let (fut_val, fut_tid) = fut_handle.await.unwrap();
                assert_eq!(cb_val + fut_val, 30);
                (std::thread::current().id(), cb_tid, fut_tid)
            }
        })
        .await
        .unwrap();

    assert_eq!(parent_tid, cb_tid);
    assert_eq!(parent_tid, fut_tid);
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0, 0]);
}

/// Dropping all `LocalPoolHandle`s while pinned tasks are still in flight lets
/// those tasks complete and deliver their outputs before workers shut down.
#[tokio::test]
async fn pool_drop_allows_inflight_tasks_to_complete() {
    const TASKS: usize = 8;
    let pool = task::LocalPoolHandle::new(2);
    let started = Arc::new(Barrier::new(TASKS + 1));
    let (gate_tx, gate_rx) = tokio::sync::watch::channel(false);

    let mut handles = Vec::with_capacity(TASKS);
    for i in 0..TASKS {
        let started = Arc::clone(&started);
        let mut gate_rx = gate_rx.clone();
        handles.push(pool.spawn_pinned(move || {
            let local = Rc::new(i);
            async move {
                started.wait().await;
                let _ = gate_rx.wait_for(|&open| open).await;
                *local
            }
        }));
    }

    // Ensure all tasks are running on the workers before dropping the pool.
    started.wait().await;
    drop(pool);
    let _ = gate_tx.send(true);

    for (i, handle) in handles.into_iter().enumerate() {
        assert_eq!(handle.await.unwrap(), i);
    }
}

/// Dropping the last `LocalPoolHandle` from inside a pinned task (after
/// spawning another task) completes cleanly without deadlocking.
#[tokio::test]
async fn pool_drop_from_inside_pinned_task() {
    let pool = task::LocalPoolHandle::new(1);
    let pool_clone = pool.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();

    let outer = pool.spawn_pinned(move || async move {
        let inner = pool_clone.spawn_pinned_by_idx(
            || {
                let local = Rc::new(42usize);
                async move {
                    tokio::task::yield_now().await;
                    *local
                }
            },
            0,
        );
        drop(pool_clone);
        let _ = tx.send(inner);
    });

    drop(pool);

    outer.await.unwrap();
    let inner = rx.await.unwrap();
    assert_eq!(inner.await.unwrap(), 42);
}

/// Panics in the callback, during polling, or inside the future's destructor
/// (on completion or abort) all decrement task load, never double-drop, and
/// leave the worker usable.
#[tokio::test]
#[cfg(panic = "unwind")]
async fn panic_in_callback_poll_and_drop_keeps_load_consistent() {
    use std::sync::atomic::AtomicUsize;

    struct PanicOnDrop {
        drops: Arc<AtomicUsize>,
        _not_send: Rc<()>,
    }

    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
            panic!("drop panic");
        }
    }

    let pool = task::LocalPoolHandle::new(1);

    // 1. Panic in callback
    let err = pool
        .spawn_pinned(|| {
            panic!("callback panic");
            #[allow(unreachable_code)]
            async {}
        })
        .await
        .unwrap_err();
    assert!(err.is_panic());
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);

    // 2. Panic in poll
    let err = pool
        .spawn_pinned(|| {
            let local = Rc::new(1usize);
            async move {
                let _ = *local;
                panic!("poll panic");
            }
        })
        .await
        .unwrap_err();
    assert!(err.is_panic());
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);

    struct ReadyThenPanicOnDrop {
        drops: Arc<AtomicUsize>,
        _not_send: Rc<()>,
    }

    impl std::future::Future for ReadyThenPanicOnDrop {
        type Output = ();

        fn poll(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            std::task::Poll::Ready(())
        }
    }

    impl Drop for ReadyThenPanicOnDrop {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
            panic!("ready drop panic");
        }
    }

    // 3. Panic in future's Drop immediately after Poll::Ready
    let drops_ready = Arc::new(AtomicUsize::new(0));
    let drops_ready_clone = Arc::clone(&drops_ready);
    let err = pool
        .spawn_pinned(move || ReadyThenPanicOnDrop {
            drops: drops_ready_clone,
            _not_send: Rc::new(()),
        })
        .await
        .unwrap_err();
    assert!(err.is_panic());
    assert_eq!(drops_ready.load(Ordering::SeqCst), 1);
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);

    // 4. Panic in Drop on abort
    let drops_abort = Arc::new(AtomicUsize::new(0));
    let drops_abort_clone = Arc::clone(&drops_abort);
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let handle = pool.spawn_pinned(move || {
        let guard = PanicOnDrop {
            drops: drops_abort_clone,
            _not_send: Rc::new(()),
        };
        async move {
            let _guard = guard;
            let _ = started_tx.send(());
            futures::future::pending::<()>().await;
        }
    });
    started_rx.await.unwrap();
    handle.abort();
    let err = handle.await.unwrap_err();
    assert!(err.is_panic());
    assert_eq!(drops_abort.load(Ordering::SeqCst), 1);
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);

    // Worker remains healthy after all panics.
    assert_eq!(pool.spawn_pinned(|| async { 99usize }).await.unwrap(), 99);
    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0]);
}

/// Concurrent spawns and aborts across multiple workers maintain accurate task
/// load counts.
#[tokio::test]
async fn concurrent_spawns_and_aborts_keep_task_counts_consistent() {
    let workers = if cfg!(miri) { 2 } else { 4 };
    let tasks_per_producer = if cfg!(miri) { 6 } else { 100 };
    let producers = if cfg!(miri) { 2 } else { 4 };

    let pool = task::LocalPoolHandle::new(workers);
    let mut producer_handles = Vec::with_capacity(producers);

    for p in 0..producers {
        let pool = pool.clone();
        producer_handles.push(tokio::spawn(async move {
            let mut handles = Vec::with_capacity(tasks_per_producer);
            for i in 0..tasks_per_producer {
                let val = p * tasks_per_producer + i;
                let h = if i % 3 == 0 {
                    pool.spawn_pinned_by_idx(
                        move || {
                            let rc = Rc::new(val);
                            async move { *rc }
                        },
                        i % workers,
                    )
                } else if i % 3 == 1 {
                    pool.spawn_pinned(move || {
                        let rc = Rc::new(val);
                        async move {
                            tokio::task::yield_now().await;
                            *rc
                        }
                    })
                } else {
                    let h = pool.spawn_pinned(move || {
                        let rc = Rc::new(val);
                        async move {
                            tokio::task::yield_now().await;
                            *rc
                        }
                    });
                    h.abort();
                    h
                };
                handles.push((i % 3 == 2, val, h));
            }

            for (aborted, expected, h) in handles {
                match h.await {
                    Ok(v) => assert_eq!(v, expected),
                    Err(e) => {
                        assert!(aborted, "unexpected error on non-aborted task: {e}");
                        assert!(e.is_cancelled());
                    }
                }
            }
        }));
    }

    for ph in producer_handles {
        ph.await.unwrap();
    }

    assert_eq!(pool.get_task_loads_for_each_worker(), vec![0; workers]);
}
