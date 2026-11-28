//! Tests for the instrumentation of tasks spawned by tokio-util.
//!
//! These tests live in their own binary because `tracing` caches callsite
//! interest per process, so spans created by concurrently running tests
//! can disable the callsites a mock subscriber is waiting for.
#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, target_has_atomic = "64", feature = "rt"))]
#![cfg(not(target_os = "wasi"))] // Wasi doesn't support threads

use tokio::runtime::Handle;
use tokio::task::LocalSet;
#[cfg(feature = "join-map")]
use tokio_util::task::JoinMap;
use tokio_util::task::{JoinQueue, LocalPoolHandle, TaskTracker};
use tracing_mock::{expect, span::NewSpan, subscriber};

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run_test<F: std::future::Future>(f: impl FnOnce() -> F) -> F::Output {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f())
}

/// Tasks spawned with `spawn_pinned` and `spawn_pinned_by_idx` record their
/// caller as the spawn location (shown by tokio-console), not tokio-util.
#[test]
fn spawn_pinned_loc_recorded() {
    run_test(|| async {
        let pool = LocalPoolHandle::new(1);
        // The worker thread creates a task span when it starts its runtime. Wait
        // for that before installing a mock subscriber: a callsite first reached
        // on a thread without the mock would be disabled for all threads.
        pool.spawn_pinned(|| async {}).await.unwrap();

        expect_task_at(line!(), || pool.spawn_pinned(|| async {}))
            .await
            .unwrap();
        expect_task_at(line!(), || pool.spawn_pinned_by_idx(|| async {}, 0))
            .await
            .unwrap();
    });
}

#[test]
fn join_queue_loc_recorded() {
    run_test(|| async {
        let h = Handle::current();
        let local = LocalSet::new();
        let mut queue = JoinQueue::new();

        expect_task_at(line!(), || queue.spawn(async {}));
        expect_task_at(line!(), || queue.spawn_on(async {}, &h));
        expect_blocking_at(line!(), || queue.spawn_blocking(|| {}));
        expect_blocking_at(line!(), || queue.spawn_blocking_on(|| {}, &h));

        local
            .run_until(async {
                expect_task_at(line!(), || queue.spawn_local(async {}));
                while let Some(res) = queue.join_next().await {
                    res.unwrap();
                }
            })
            .await;

        let futs = [futures::future::ready(()), futures::future::ready(())];
        let mut from_iter_queue: JoinQueue<()> =
            expect_tasks_at(2, line!(), || std::iter::FromIterator::from_iter(futs));
        while let Some(res) = from_iter_queue.join_next().await {
            res.unwrap();
        }
    });
}

#[test]
fn task_tracker_loc_recorded() {
    run_test(|| async {
        let h = Handle::current();
        let local = LocalSet::new();
        let tracker = TaskTracker::new();

        expect_task_at(line!(), || tracker.spawn(async {}))
            .await
            .unwrap();
        expect_task_at(line!(), || tracker.spawn_on(async {}, &h))
            .await
            .unwrap();
        expect_blocking_at(line!(), || tracker.spawn_blocking(|| {}))
            .await
            .unwrap();
        expect_blocking_at(line!(), || tracker.spawn_blocking_on(|| {}, &h))
            .await
            .unwrap();

        let local_handle = expect_task_at(line!(), || tracker.spawn_local_on(async {}, &local));
        local
            .run_until(async {
                local_handle.await.unwrap();
                expect_task_at(line!(), || tracker.spawn_local(async {}))
                    .await
                    .unwrap();
            })
            .await;

        tracker.close();
        tracker.wait().await;
    });
}

#[cfg(feature = "join-map")]
#[test]
fn join_map_loc_recorded() {
    run_test(|| async {
        let h = Handle::current();
        let local = LocalSet::new();
        let mut map = JoinMap::new();

        expect_task_at(line!(), || map.spawn(1, async {}));
        expect_task_at(line!(), || map.spawn_on(2, async {}, &h));
        expect_task_at(line!(), || map.spawn_local_on(3, async {}, &local));
        expect_blocking_at(line!(), || map.spawn_blocking(4, || {}));
        expect_blocking_at(line!(), || map.spawn_blocking_on(5, || {}, &h));

        local
            .run_until(async {
                expect_task_at(line!(), || map.spawn_local(6, async {}));
                while let Some((_, res)) = map.join_next().await {
                    res.unwrap();
                }
            })
            .await;
    });
}

fn expect_task_span_at(line: u32, target: &'static str) -> NewSpan {
    let location = expect::field("loc.file")
        .with_value(&file!())
        .and(expect::field("loc.line").with_value(&line));
    expect::span()
        .named("runtime.spawn")
        .with_target(target)
        .with_fields(location)
}

/// Asserts that `spawn` creates a task span located at `line` of this file.
///
/// Each call uses its own mock subscriber with a single expectation, as worker
/// threads may enter the span of a spawned task concurrently.
fn expect_task_at<R>(line: u32, spawn: impl FnOnce() -> R) -> R {
    expect_spawned_n_at(1, line, "tokio::task", spawn)
}

fn expect_tasks_at<R>(count: usize, line: u32, spawn: impl FnOnce() -> R) -> R {
    expect_spawned_n_at(count, line, "tokio::task", spawn)
}

fn expect_blocking_at<R>(line: u32, spawn: impl FnOnce() -> R) -> R {
    expect_spawned_n_at(1, line, "tokio::task::blocking", spawn)
}

fn expect_spawned_n_at<R>(
    count: usize,
    line: u32,
    target: &'static str,
    spawn: impl FnOnce() -> R,
) -> R {
    let mut mock = subscriber::mock()
        // Skip the spans of any channels created before spawning.
        .with_filter(move |meta| meta.target() == target);
    for _ in 0..count {
        mock = mock.new_span(expect_task_span_at(line, target));
    }
    let (subscriber, handle) = mock.run_with_handle();

    let res = tracing::subscriber::with_default(subscriber, spawn);
    handle.assert_finished();
    res
}
