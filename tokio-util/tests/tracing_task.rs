//! Tests for the instrumentation of tasks spawned by tokio-util.
//!
//! These tests live in their own binary because `tracing` caches callsite
//! interest per process, so spans created by concurrently running tests
//! can disable the callsites a mock subscriber is waiting for.
#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, target_has_atomic = "64", feature = "rt"))]
#![cfg(not(target_os = "wasi"))] // Wasi doesn't support threads

use tokio::task::JoinHandle;
use tokio_util::task::LocalPoolHandle;
use tracing_mock::{expect, subscriber};

/// Tasks spawned with `spawn_pinned` and `spawn_pinned_by_idx` record their
/// caller as the spawn location (shown by tokio-console), not tokio-util.
#[tokio::test]
async fn spawn_pinned_loc_recorded() {
    let pool = LocalPoolHandle::new(1);
    // The worker thread creates a task span when it starts its runtime. Wait
    // for that before installing a mock subscriber: a callsite first reached
    // on a thread without the mock would be disabled for all threads.
    pool.spawn_pinned(|| async {}).await.unwrap();

    expect_spawned_at(line!(), || pool.spawn_pinned(|| async {}))
        .await
        .unwrap();
    expect_spawned_at(line!(), || pool.spawn_pinned_by_idx(|| async {}, 0))
        .await
        .unwrap();
}

/// Asserts that `spawn` creates a task span located at `line` of this file.
///
/// Each call uses its own mock subscriber with a single expectation, as the
/// worker thread enters the span of a spawned task concurrently.
fn expect_spawned_at(line: u32, spawn: impl FnOnce() -> JoinHandle<()>) -> JoinHandle<()> {
    let location = expect::field("loc.file")
        .with_value(&file!())
        .and(expect::field("loc.line").with_value(&line));
    let task_span = expect::span().named("runtime.spawn").with_fields(location);
    let (subscriber, handle) = subscriber::mock()
        // Skip the spans of the oneshot channel that is created first.
        .with_filter(|meta| meta.target() == "tokio::task")
        .new_span(task_span)
        .run_with_handle();

    let task = tracing::subscriber::with_default(subscriber, spawn);
    handle.assert_finished();
    task
}
