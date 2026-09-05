#![cfg(all(
    tokio_unstable,
    feature = "taskdump",
    target_os = "linux",
    any(
        target_arch = "aarch64",
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "s390x"
    )
))]

use std::hint::black_box;
use tokio::runtime::{self, Handle};

#[inline(never)]
async fn a() {
    black_box(b()).await
}

#[inline(never)]
async fn b() {
    black_box(c()).await
}

#[inline(never)]
async fn c() {
    loop {
        black_box(tokio::task::yield_now()).await
    }
}

#[test]
fn current_thread() {
    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    async fn dump() {
        let handle = Handle::current();
        let dump = handle.dump().await;

        let tasks: Vec<_> = dump.tasks().iter().collect();

        assert_eq!(tasks.len(), 3);

        for task in tasks {
            let id = task.id();
            let trace = task.trace().to_string();
            eprintln!("\n\n{id}:\n{trace}\n\n");
            assert!(trace.contains("dump::a"));
            assert!(trace.contains("dump::b"));
            assert!(trace.contains("dump::c"));
            assert!(trace.contains("tokio::task::yield_now"));
        }
    }

    rt.block_on(async {
        tokio::select!(
            biased;
            _ = tokio::spawn(a()) => {},
            _ = tokio::spawn(a()) => {},
            _ = tokio::spawn(a()) => {},
            _ = dump() => {},
        );
    });
}

#[test]
fn multi_thread() {
    let rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(3)
        .build()
        .unwrap();

    async fn dump() {
        let handle = Handle::current();
        let dump = handle.dump().await;

        let tasks: Vec<_> = dump.tasks().iter().collect();

        assert_eq!(tasks.len(), 3);

        for task in tasks {
            let id = task.id();
            let trace = task.trace().to_string();
            eprintln!("\n\n{id}:\n{trace}\n\n");
            assert!(trace.contains("dump::a"));
            assert!(trace.contains("dump::b"));
            assert!(trace.contains("dump::c"));
            assert!(trace.contains("tokio::task::yield_now"));
        }
    }

    rt.block_on(async {
        tokio::select!(
            biased;
            _ = tokio::spawn(a()) => {},
            _ = tokio::spawn(a()) => {},
            _ = tokio::spawn(a()) => {},
            _ = dump() => {},
        );
    });
}

/// Regression tests for #6035.
///
/// These tests ensure that dumping will not deadlock if a future completes
/// during a trace.
mod future_completes_during_trace {
    use super::*;

    use core::future::{poll_fn, Future};

    /// A future that completes only during a trace.
    fn complete_during_trace() -> impl Future<Output = ()> + Send {
        use std::task::Poll;
        poll_fn(|cx| {
            if Handle::is_tracing() {
                Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
    }

    #[test]
    fn current_thread() {
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        async fn dump() {
            let handle = Handle::current();
            let _dump = handle.dump().await;
        }

        rt.block_on(async {
            let _ = tokio::join!(tokio::spawn(complete_during_trace()), dump());
        });
    }

    #[test]
    fn multi_thread() {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        async fn dump() {
            let handle = Handle::current();
            let _dump = handle.dump().await;
            tokio::task::yield_now().await;
        }

        rt.block_on(async {
            let _ = tokio::join!(tokio::spawn(complete_during_trace()), dump());
        });
    }
}

/// Regression test for #6051.
///
/// This test ensures that tasks notified outside of a worker will not be
/// traced, since doing so will un-set their notified bit prior to them being
/// run and panic.
#[test]
fn notified_during_tracing() {
    let rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(3)
        .build()
        .unwrap();

    let timeout = async {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    };

    let timer = rt.spawn(async {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_nanos(1)).await;
        }
    });

    let dump = async {
        loop {
            let handle = Handle::current();
            let _dump = handle.dump().await;
        }
    };

    rt.block_on(async {
        tokio::select!(
            biased;
            _ = timeout => {},
            _ = timer => {},
            _ = dump => {},
        );
    });
}

/// Regression test for an arrival leak in the task-dump barrier's
/// `wait_timeout` (`loom::std::barrier`).
///
/// When a worker is wedged and never reaches the dump barrier, the healthy
/// workers repeatedly time out waiting for it. If a timed-out arrival is not
/// rolled back, `count` (reset to zero only when a leader is elected) leaks,
/// and a later barrier round crosses `num_threads` with fewer than
/// `num_threads` workers actually present. That elects a spurious leader which
/// traces tasks without the exclusive access the barrier guarantees, un-setting
/// the notified bit of a task another worker is about to run and panicking that
/// worker via `assert!(next.is_notified())` — the same assert as #6051.
///
/// This drives that scenario and ensures the runtime survives. In a debug build
/// the panic aborts the whole process via the worker's abort-on-panic guard, so
/// this test crashes without the fix and passes with it.
#[test]
fn wedged_worker_during_tracing() {
    let rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .unwrap();

    // Permanently occupy one worker so only 3 of the 4 workers ever reach the
    // dump barrier — the condition that leaks `count` without the fix.
    rt.spawn(async {
        loop {
            std::hint::spin_loop();
        }
    });

    // Keep the remaining workers continuously polling tasks, each burning a
    // little CPU before yielding, to widen the window in which a spurious leader
    // would trace a task another worker is actively polling.
    for _ in 0..12 {
        rt.spawn(async {
            loop {
                let deadline = std::time::Instant::now() + std::time::Duration::from_micros(20);
                while std::time::Instant::now() < deadline {
                    std::hint::spin_loop();
                }
                tokio::task::yield_now().await;
            }
        });
    }

    // Continuously request dumps. With a permanently wedged worker a dump never
    // completes, so this future is expected never to resolve.
    let handle = rt.handle().clone();
    rt.spawn(async move {
        loop {
            let _ = handle.dump().await;
        }
    });

    // Bound the test on the *test thread's* wall clock rather than a runtime
    // timer: the busy workers above would starve an in-runtime timer, and the
    // point is only to give the buggy spurious-leader race time to fire (it
    // aborts a worker within a fraction of a second without the fix).
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Tear down without joining the wedged worker, which never returns:
    // `shutdown_background` is `shutdown_timeout(0)`, so it skips all joins.
    rt.shutdown_background();
}
