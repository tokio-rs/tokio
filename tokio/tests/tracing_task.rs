//! Tests for task instrumentation.
//!
//! These tests ensure that the instrumentation for task spawning and task
//! lifecycles is correct.
#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "tracing", target_has_atomic = "64"))]

use std::{mem, time::Duration};

use tokio::task;
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

#[test]
fn task_spawn_creates_span() {
    run_test(|| async {
        let task_span = expect::span()
            .named("runtime.spawn")
            .with_target("tokio::task");

        let (subscriber, handle) = subscriber::mock()
            .new_span(&task_span)
            .enter(&task_span)
            .exit(&task_span)
            // The task span is entered once more when it gets dropped
            .enter(&task_span)
            .exit(&task_span)
            .drop_span(task_span)
            .run_with_handle();

        {
            let _guard = tracing::subscriber::set_default(subscriber);
            tokio::spawn(futures::future::ready(()))
                .await
                .expect("failed to await join handle");
        }

        handle.assert_finished();
    });
}

#[test]
fn task_spawn_loc_file_recorded() {
    run_test(|| async {
        let task_span = expect::span()
            .named("runtime.spawn")
            .with_target("tokio::task")
            .with_fields(expect::field("loc.file").with_value(&file!()));

        let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

        {
            let _guard = tracing::subscriber::set_default(subscriber);

            tokio::spawn(futures::future::ready(()))
                .await
                .expect("failed to await join handle");
        }

        handle.assert_finished();
    });
}

#[test]
fn task_builder_name_recorded() {
    run_test(|| async {
        let task_span = expect_task_named("test-task");

        let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

        {
            let _guard = tracing::subscriber::set_default(subscriber);
            task::Builder::new()
                .name("test-task")
                .spawn(futures::future::ready(()))
                .unwrap()
                .await
                .expect("failed to await join handle");
        }

        handle.assert_finished();
    });
}

#[test]
fn task_builder_loc_file_recorded() {
    run_test(|| async {
        let task_span = expect::span()
            .named("runtime.spawn")
            .with_target("tokio::task")
            .with_fields(expect::field("loc.file").with_value(&file!()));

        let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

        {
            let _guard = tracing::subscriber::set_default(subscriber);

            task::Builder::new()
                .spawn(futures::future::ready(()))
                .unwrap()
                .await
                .expect("failed to await join handle");
        }

        handle.assert_finished();
    });
}

#[test]
fn task_spawn_sizes_recorded() {
    run_test(|| async {
        let future = futures::future::ready(());
        let size = mem::size_of_val(&future) as u64;

        let task_span = expect::span()
            .named("runtime.spawn")
            .with_target("tokio::task")
            // TODO(hds): check that original_size.bytes is NOT recorded when this can be done in
            // tracing-mock without listing every other field.
            .with_fields(expect::field("size.bytes").with_value(&size));

        let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

        {
            let _guard = tracing::subscriber::set_default(subscriber);

            task::Builder::new()
                .spawn(future)
                .unwrap()
                .await
                .expect("failed to await join handle");
        }

        handle.assert_finished();
    });
}

#[test]
fn task_big_spawn_sizes_recorded() {
    run_test(|| async {
        let future = {
            async fn big<const N: usize>() {
                let mut a = [0_u8; N];
                for (idx, item) in a.iter_mut().enumerate() {
                    *item = (idx % 256) as u8;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
                for (idx, item) in a.iter_mut().enumerate() {
                    assert_eq!(*item, (idx % 256) as u8);
                }
            }

            // This is larger than the release auto-boxing threshold
            big::<20_000>()
        };

        fn boxed_size<T>(_: &T) -> usize {
            mem::size_of::<Box<T>>()
        }
        let size = mem::size_of_val(&future) as u64;
        let boxed_size = boxed_size(&future);

        let task_span = expect::span()
            .named("runtime.spawn")
            .with_target("tokio::task")
            .with_fields(
                expect::field("size.bytes")
                    .with_value(&boxed_size)
                    .and(expect::field("original_size.bytes").with_value(&size)),
            );

        let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

        {
            let _guard = tracing::subscriber::set_default(subscriber);

            task::Builder::new()
                .spawn(future)
                .unwrap()
                .await
                .expect("failed to await join handle");
        }

        handle.assert_finished();
    });
}

/// Expect a task with name
///
/// This is a convenience function to create the expectation for a new task
/// with the `task.name` field set to the provided name.
fn expect_task_named(name: &str) -> NewSpan {
    expect::span()
        .named("runtime.spawn")
        .with_target("tokio::task")
        .with_fields(
            expect::field("task.name").with_value(&tracing::field::debug(format_args!("{name}"))),
        )
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

fn expect_task_at<R>(line: u32, spawn: impl FnOnce() -> R) -> R {
    expect_spawned_n_at(1, line, "tokio::task", spawn)
}

fn expect_tasks_at<R>(count: usize, line: u32, spawn: impl FnOnce() -> R) -> R {
    expect_spawned_n_at(count, line, "tokio::task", spawn)
}

#[cfg(not(target_os = "wasi"))]
fn expect_blocking_at<R>(line: u32, spawn: impl FnOnce() -> R) -> R {
    expect_spawned_n_at(1, line, "tokio::task::blocking", spawn)
}

fn expect_spawned_n_at<R>(
    count: usize,
    line: u32,
    target: &'static str,
    spawn: impl FnOnce() -> R,
) -> R {
    let mut mock = subscriber::mock().with_filter(move |meta| meta.target() == target);
    for _ in 0..count {
        mock = mock.new_span(expect_task_span_at(line, target));
    }
    let (subscriber, handle) = mock.run_with_handle();

    let res = tracing::subscriber::with_default(subscriber, spawn);
    handle.assert_finished();
    res
}

#[test]
fn task_spawn_loc_recorded() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let handle_task = expect_task_at(line!(), || rt.spawn(async {}));

    rt.block_on(async {
        handle_task.await.unwrap();

        let h = tokio::runtime::Handle::current();

        expect_task_at(line!(), || tokio::spawn(async {}))
            .await
            .unwrap();
        expect_task_at(line!(), || h.spawn(async {})).await.unwrap();
        let b = task::Builder::new();
        expect_task_at(line!(), || b.spawn(async {}).unwrap())
            .await
            .unwrap();
        let b = task::Builder::new();
        expect_task_at(line!(), || b.spawn_on(async {}, &h).unwrap())
            .await
            .unwrap();
    });
}

#[test]
fn task_spawn_local_loc_recorded() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let local = task::LocalSet::new();
        let b = task::Builder::new();

        let h1 = expect_task_at(line!(), || local.spawn_local(async {}));
        let h2 = expect_task_at(line!(), || b.spawn_local_on(async {}, &local).unwrap());

        local
            .run_until(async {
                h1.await.unwrap();
                h2.await.unwrap();

                let b = task::Builder::new();
                expect_task_at(line!(), || task::spawn_local(async {}))
                    .await
                    .unwrap();
                expect_task_at(line!(), || b.spawn_local(async {}).unwrap())
                    .await
                    .unwrap();
            })
            .await;
    });

    let local_rt = tokio::runtime::LocalRuntime::new().unwrap();
    let h = expect_task_at(line!(), || local_rt.spawn_local(async {}));
    local_rt.block_on(async {
        h.await.unwrap();

        let b = task::Builder::new();
        expect_task_at(line!(), || task::spawn_local(async {}))
            .await
            .unwrap();
        expect_task_at(line!(), || b.spawn_local(async {}).unwrap())
            .await
            .unwrap();
    });
}

#[test]
fn task_join_set_loc_recorded() {
    run_test(|| async {
        let h = tokio::runtime::Handle::current();
        let local = task::LocalSet::new();
        let mut set = task::JoinSet::new();

        expect_task_at(line!(), || set.spawn(async {}));
        expect_task_at(line!(), || set.spawn_on(async {}, &h));
        expect_task_at(line!(), || set.spawn_local_on(async {}, &local));
        expect_task_at(line!(), || set.build_task().spawn(async {}).unwrap());
        expect_task_at(line!(), || set.build_task().spawn_on(async {}, &h).unwrap());
        let b = set.build_task();
        expect_task_at(line!(), || b.spawn_local_on(async {}, &local)).unwrap();

        #[cfg(not(target_os = "wasi"))]
        {
            expect_blocking_at(line!(), || set.spawn_blocking(|| {}));
            expect_blocking_at(line!(), || set.spawn_blocking_on(|| {}, &h));
            expect_blocking_at(line!(), || set.build_task().spawn_blocking(|| {}).unwrap());
            expect_blocking_at(line!(), || set.build_task().spawn_blocking_on(|| {}, &h)).unwrap();
        }

        local
            .run_until(async {
                expect_task_at(line!(), || set.spawn_local(async {}));
                expect_task_at(line!(), || set.build_task().spawn_local(async {}).unwrap());

                while let Some(res) = set.join_next().await {
                    res.unwrap();
                }
            })
            .await;

        let futs = [futures::future::ready(()), futures::future::ready(())];
        let mut from_iter_set: task::JoinSet<()> =
            expect_tasks_at(2, line!(), || std::iter::FromIterator::from_iter(futs));
        let futs = [futures::future::ready(()), futures::future::ready(())];
        expect_tasks_at(2, line!(), || from_iter_set.extend(futs));
        while let Some(res) = from_iter_set.join_next().await {
            res.unwrap();
        }
    });
}

#[cfg(not(target_os = "wasi"))]
#[test]
fn task_spawn_blocking_loc_recorded() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let h = expect_blocking_at(line!(), || rt.spawn_blocking(|| {}));

    rt.block_on(async {
        h.await.unwrap();

        let h = tokio::runtime::Handle::current();

        expect_blocking_at(line!(), || task::spawn_blocking(|| {}))
            .await
            .unwrap();
        expect_blocking_at(line!(), || h.spawn_blocking(|| {}))
            .await
            .unwrap();
        let b = task::Builder::new();
        expect_blocking_at(line!(), || b.spawn_blocking(|| {}).unwrap())
            .await
            .unwrap();
        let b = task::Builder::new();
        expect_blocking_at(line!(), || b.spawn_blocking_on(|| {}, &h).unwrap())
            .await
            .unwrap();
    });

    let local_rt = tokio::runtime::LocalRuntime::new().unwrap();
    let h = expect_blocking_at(line!(), || local_rt.spawn_blocking(|| {}));
    local_rt.block_on(async {
        h.await.unwrap();
    });
}

#[test]
fn block_on_loc_recorded() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    expect_task_at(line!(), || rt.block_on(async {}));

    let handle = rt.handle();
    expect_task_at(line!(), || handle.block_on(async {}));

    let local = task::LocalSet::new();
    expect_task_at(line!(), || local.block_on(&rt, async {}));

    let local_rt = tokio::runtime::LocalRuntime::new().unwrap();
    expect_task_at(line!(), || local_rt.block_on(async {}));
}
