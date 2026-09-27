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

/// Regression tests for the scheduler's idle bookkeeping across a dump.
///
/// Requesting a dump wakes every worker thread directly, without going through
/// the multi-threaded scheduler's idle bookkeeping. A worker that was parked
/// when the dump began must therefore perform the logical unpark itself, or the
/// bookkeeping is corrupted the next time it parks: it is recorded as a sleeper
/// twice and the count of unparked workers underflows. After that, the runtime
/// believes every worker is busy and never notifies one of newly spawned work.
mod dump_of_parked_workers {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::{mpsc, Arc};
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Observes the worker threads entering and leaving the park path.
    #[derive(Default)]
    struct ParkState {
        /// Number of workers currently in the park path.
        parked: AtomicUsize,
        /// Number of times a worker has entered the park path.
        entries: AtomicUsize,
    }

    impl ParkState {
        fn wait_until(&self, mut condition: impl FnMut(&Self) -> bool, message: &str) {
            let deadline = Instant::now() + TIMEOUT;
            while !condition(self) {
                assert!(Instant::now() < deadline, "{message}");
                std::thread::yield_now();
            }
        }

        fn wait_for_all_workers_parked(&self, worker_threads: usize) {
            self.wait_until(
                |state| state.parked.load(SeqCst) == worker_threads,
                "not every worker reached the park path",
            );
        }
    }

    fn assert_dump_preserves_idle_bookkeeping(worker_threads: usize) {
        let park_state = Arc::new(ParkState::default());

        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(worker_threads)
            .on_thread_park({
                let park_state = park_state.clone();
                move || {
                    park_state.entries.fetch_add(1, SeqCst);
                    park_state.parked.fetch_add(1, SeqCst);
                }
            })
            .on_thread_unpark({
                let park_state = park_state.clone();
                move || {
                    park_state.parked.fetch_sub(1, SeqCst);
                }
            })
            .build()
            .unwrap();

        // Dump only once every worker is idle. That is the state in which the
        // dump wakes worker threads that the scheduler still counts as parked.
        park_state.wait_for_all_workers_parked(worker_threads);
        let entries_before_dump = park_state.entries.load(SeqCst);

        rt.block_on(rt.handle().dump());

        // Every worker leaves the park path to be traced, then parks again.
        park_state.wait_until(
            |state| state.entries.load(SeqCst) >= entries_before_dump + worker_threads,
            "not every worker was woken for the dump",
        );
        park_state.wait_for_all_workers_parked(worker_threads);

        let (tx, rx) = mpsc::channel();
        rt.spawn(async move {
            let _ = tx.send(());
        });

        assert!(
            rx.recv_timeout(TIMEOUT).is_ok(),
            "the runtime did not run a task spawned after a dump"
        );
    }

    #[test]
    fn one_worker() {
        assert_dump_preserves_idle_bookkeeping(1);
    }

    #[test]
    fn two_workers() {
        assert_dump_preserves_idle_bookkeeping(2);
    }

    #[test]
    fn many_workers() {
        assert_dump_preserves_idle_bookkeeping(8);
    }
}
