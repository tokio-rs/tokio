#![cfg(all(
    tokio_unstable,
    tokio_taskdump,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
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
            let trace = task.trace().to_string();
            eprintln!("\n\n{trace}\n\n");
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
            let trace = task.trace().to_string();
            eprintln!("\n\n{trace}\n\n");
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
        }

        rt.block_on(async {
            let _ = tokio::join!(tokio::spawn(complete_during_trace()), dump());
        });
    }
}
