#![warn(rust_2018_idioms)]

use futures::future::pending;
use std::rc::Rc;
#[cfg(tokio_unstable)]
use tokio::runtime::LocalRuntime;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tokio_test::{assert_pending, assert_ready, task};
use tokio_util::task::TaskTracker;

#[test]
fn open_close() {
    let tracker = TaskTracker::new();
    assert!(!tracker.is_closed());
    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);

    tracker.close();
    assert!(tracker.is_closed());
    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);

    tracker.reopen();
    assert!(!tracker.is_closed());
    tracker.reopen();
    assert!(!tracker.is_closed());

    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);

    tracker.close();
    assert!(tracker.is_closed());
    tracker.close();
    assert!(tracker.is_closed());

    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);
}

#[test]
fn token_len() {
    let tracker = TaskTracker::new();

    let mut tokens = Vec::new();
    for i in 0..10 {
        assert_eq!(tracker.len(), i);
        tokens.push(tracker.token());
    }

    assert!(!tracker.is_empty());
    assert_eq!(tracker.len(), 10);

    for (i, token) in tokens.into_iter().enumerate() {
        drop(token);
        assert_eq!(tracker.len(), 9 - i);
    }
}

#[test]
fn notify_immediately() {
    let tracker = TaskTracker::new();
    tracker.close();

    let mut wait = task::spawn(tracker.wait());
    assert_ready!(wait.poll());
}

#[test]
fn notify_immediately_on_reopen() {
    let tracker = TaskTracker::new();
    tracker.close();

    let mut wait = task::spawn(tracker.wait());
    tracker.reopen();
    assert_ready!(wait.poll());
}

#[test]
fn notify_on_close() {
    let tracker = TaskTracker::new();

    let mut wait = task::spawn(tracker.wait());

    assert_pending!(wait.poll());
    tracker.close();
    assert_ready!(wait.poll());
}

#[test]
fn notify_on_close_reopen() {
    let tracker = TaskTracker::new();

    let mut wait = task::spawn(tracker.wait());

    assert_pending!(wait.poll());
    tracker.close();
    tracker.reopen();
    assert_ready!(wait.poll());
}

#[test]
fn notify_on_last_task() {
    let tracker = TaskTracker::new();
    tracker.close();
    let token = tracker.token();

    let mut wait = task::spawn(tracker.wait());
    assert_pending!(wait.poll());
    drop(token);
    assert_ready!(wait.poll());
}

#[test]
fn notify_on_last_task_respawn() {
    let tracker = TaskTracker::new();
    tracker.close();
    let token = tracker.token();

    let mut wait = task::spawn(tracker.wait());
    assert_pending!(wait.poll());
    drop(token);
    let token2 = tracker.token();
    assert_ready!(wait.poll());
    drop(token2);
}

#[test]
fn no_notify_on_respawn_if_open() {
    let tracker = TaskTracker::new();
    let token = tracker.token();

    let mut wait = task::spawn(tracker.wait());
    assert_pending!(wait.poll());
    drop(token);
    let token2 = tracker.token();
    assert_pending!(wait.poll());
    drop(token2);
}

#[test]
fn close_during_exit() {
    const ITERS: usize = 5;

    for close_spot in 0..=ITERS {
        let tracker = TaskTracker::new();
        let tokens: Vec<_> = (0..ITERS).map(|_| tracker.token()).collect();

        let mut wait = task::spawn(tracker.wait());

        for (i, token) in tokens.into_iter().enumerate() {
            assert_pending!(wait.poll());
            if i == close_spot {
                tracker.close();
                assert_pending!(wait.poll());
            }
            drop(token);
        }

        if close_spot == ITERS {
            assert_pending!(wait.poll());
            tracker.close();
        }

        assert_ready!(wait.poll());
    }
}

#[test]
fn notify_many() {
    let tracker = TaskTracker::new();

    let mut waits: Vec<_> = (0..10).map(|_| task::spawn(tracker.wait())).collect();

    for wait in &mut waits {
        assert_pending!(wait.poll());
    }

    tracker.close();

    for wait in &mut waits {
        assert_ready!(wait.poll());
    }
}

#[cfg(tokio_unstable)]
#[test]
fn local_runtime_spawn_and_wait() {
    const N: usize = 8;
    let rt = LocalRuntime::new().unwrap();

    rt.block_on(async {
        let tracker = TaskTracker::new();

        for _ in 0..5 {
            tracker.spawn(async {});
        }

        for _ in 0..N {
            tracker.spawn_on(async {}, rt.handle());
        }

        tracker.close();
        tracker.wait().await;

        assert!(tracker.is_empty());
        assert!(tracker.is_closed());
    });
}

#[cfg(tokio_unstable)]
#[test]
fn local_runtime_spawn_local() {
    const N: usize = 8;
    let rt = LocalRuntime::new().unwrap();

    rt.block_on(async {
        let tracker = TaskTracker::new();

        for _ in 0..N {
            let rc = Rc::new(());
            tracker.spawn_local(async move {
                drop(rc);
            });
        }

        tracker.close();
        tracker.wait().await;

        assert!(tracker.is_empty());
        assert!(tracker.is_closed());
    });
}

#[cfg(tokio_unstable)]
#[test]
fn local_runtime_spawn_local_on_localset() {
    const N: usize = 8;
    let rt = LocalRuntime::new().unwrap();
    let local_set = LocalSet::new();

    rt.block_on(local_set.run_until(async {
        let tracker = TaskTracker::new();

        for _ in 0..N {
            let rc = Rc::new(());
            tracker.spawn_local_on(
                async move {
                    drop(rc);
                },
                &local_set,
            );
        }

        tracker.close();
        tracker.wait().await;

        assert!(tracker.is_empty());
        assert!(tracker.is_closed());
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn drop_spawn_local_on_running_localset() {
    const N: usize = 8;

    let local = LocalSet::new();
    let tracker = TaskTracker::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<()>();

    for _i in 0..N {
        let tx = tx.clone();
        tracker.spawn_local_on(
            async move {
                pending::<()>().await;
                drop(tx);
            },
            &local,
        );
    }
    drop(tx);

    local
        .run_until(async move {
            drop(tracker);
            tokio::task::yield_now().await;

            use tokio::sync::mpsc::error::TryRecvError;

            assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
        })
        .await;
}

#[cfg(tokio_unstable)]
#[test]
fn spawn_local_after_close() {
    const N: usize = 8;

    let rt = LocalRuntime::new().unwrap();
    rt.block_on(async {
        let tracker = TaskTracker::new();

        tracker.close();

        for _ in 0..N {
            let rc = Rc::new(());
            tracker.spawn_local(async move {
                drop(rc);
            });
        }

        tracker.wait().await;

        assert!(tracker.is_closed());
        assert!(tracker.is_empty());
    });
}

#[tokio::test(flavor = "current_thread")]
async fn spawn_local_on_after_close() {
    const N: usize = 8;

    let local = LocalSet::new();
    let tracker = TaskTracker::new();

    tracker.close();

    for _ in 0..N {
        let rc = Rc::new(());
        tracker.spawn_local_on(
            async move {
                drop(rc);
            },
            &local,
        );
    }

    local
        .run_until(async move {
            tracker.wait().await;
            assert!(tracker.is_closed());
            assert!(tracker.is_empty());
        })
        .await;
}
