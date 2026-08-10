#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, target_has_atomic = "64"))]

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(feature = "schedule-latency")]
use std::time::{Duration, Instant};

use tokio::runtime::Builder;

const TASKS: usize = 8;
const ITERATIONS: usize = 64;
/// Assert that the spawn task hook always fires when set.
#[test]
fn spawn_task_hook_fires() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let ids = Arc::new(Mutex::new(HashSet::new()));
    let ids2 = Arc::clone(&ids);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(move |data| {
            #[cfg(feature = "schedule-latency")]
            assert_eq!(data.schedule_latency(), None);
            ids2.lock().unwrap().insert(data.id());

            count2.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .unwrap();

    for _ in 0..TASKS {
        runtime.spawn(std::future::pending::<()>());
    }

    let count_realized = count.load(Ordering::SeqCst);
    assert_eq!(
        TASKS, count_realized,
        "Total number of spawned task hook invocations was incorrect, expected {TASKS}, got {count_realized}"
    );

    let count_ids_realized = ids.lock().unwrap().len();

    assert_eq!(
        TASKS, count_ids_realized,
        "Total number of spawned task hook invocations was incorrect, expected {TASKS}, got {count_realized}"
    );
}

/// Assert that the terminate task hook always fires when set.
#[test]
fn terminate_task_hook_fires() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let runtime = Builder::new_current_thread()
        .on_task_terminate(move |_data| {
            #[cfg(feature = "schedule-latency")]
            assert_eq!(_data.schedule_latency(), None);
            count2.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .unwrap();

    for _ in 0..TASKS {
        runtime.spawn(std::future::ready(()));
    }

    runtime.block_on(async {
        // tick the runtime a bunch to close out tasks
        for _ in 0..ITERATIONS {
            tokio::task::yield_now().await;
        }
    });

    assert_eq!(TASKS, count.load(Ordering::SeqCst));
}

/// Test that the correct spawn location is provided to the task hooks on a
/// current thread runtime.
#[test]
fn task_hook_spawn_location_current_thread() {
    let spawns = Arc::new(AtomicUsize::new(0));
    let poll_starts = Arc::new(AtomicUsize::new(0));
    let poll_ends = Arc::new(AtomicUsize::new(0));

    let runtime = Builder::new_current_thread()
        .on_task_spawn(mk_spawn_location_hook(
            "(current_thread) on_task_spawn",
            &spawns,
        ))
        .on_before_task_poll(mk_spawn_location_hook(
            "(current_thread) on_before_task_poll",
            &poll_starts,
        ))
        .on_after_task_poll(mk_spawn_location_hook(
            "(current_thread) on_after_task_poll",
            &poll_ends,
        ))
        .build()
        .unwrap();

    let task = runtime.spawn(async move { tokio::task::yield_now().await });
    runtime.block_on(async move {
        // Spawn tasks using both `runtime.spawn(...)` and `tokio::spawn(...)`
        // to ensure the correct location is captured in both code paths.
        task.await.unwrap();
        tokio::spawn(async move {}).await.unwrap();

        // tick the runtime a bunch to close out tasks
        for _ in 0..ITERATIONS {
            tokio::task::yield_now().await;
        }
    });

    assert_eq!(spawns.load(Ordering::SeqCst), 2);
    let poll_starts = poll_starts.load(Ordering::SeqCst);
    assert!(poll_starts > 2);
    assert_eq!(poll_starts, poll_ends.load(Ordering::SeqCst));
}

/// Test that the correct spawn location is provided to the task hooks on a
/// multi-thread runtime.
///
/// Testing this separately is necessary as the spawn code paths are different
/// and we should ensure that `#[track_caller]` is passed through correctly
/// for both runtimes.
#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn task_hook_spawn_location_multi_thread() {
    let spawns = Arc::new(AtomicUsize::new(0));
    let poll_starts = Arc::new(AtomicUsize::new(0));
    let poll_ends = Arc::new(AtomicUsize::new(0));

    let runtime = Builder::new_multi_thread()
        .on_task_spawn(mk_spawn_location_hook(
            "(multi_thread) on_task_spawn",
            &spawns,
        ))
        .on_before_task_poll(mk_spawn_location_hook(
            "(multi_thread) on_before_task_poll",
            &poll_starts,
        ))
        .on_after_task_poll(mk_spawn_location_hook(
            "(multi_thread) on_after_task_poll",
            &poll_ends,
        ))
        .build()
        .unwrap();

    let task = runtime.spawn(async move { tokio::task::yield_now().await });
    runtime.block_on(async move {
        // Spawn tasks using both `runtime.spawn(...)` and `tokio::spawn(...)`
        // to ensure the correct location is captured in both code paths.
        task.await.unwrap();
        tokio::spawn(async move {}).await.unwrap();

        // tick the runtime a bunch to close out tasks
        for _ in 0..ITERATIONS {
            tokio::task::yield_now().await;
        }
    });

    // Give the runtime to shut down so that we see all the expected calls to
    // the task hooks.
    runtime.shutdown_timeout(std::time::Duration::from_secs(60));

    // Note: we "read" the counters using `fetch_add(0, SeqCst)` rather than
    // `load(SeqCst)` because read-write-modify operations are guaranteed to
    // observe the latest value, while the load is not.
    // This avoids a race that may cause test flakiness.
    assert_eq!(spawns.fetch_add(0, Ordering::SeqCst), 2);
    let poll_starts = poll_starts.fetch_add(0, Ordering::SeqCst);
    assert!(poll_starts > 2);
    assert_eq!(poll_starts, poll_ends.fetch_add(0, Ordering::SeqCst));
}

#[cfg(feature = "schedule-latency")]
#[test]
fn task_hook_schedule_latency_non_poll_callbacks() {
    let spawn_count = Arc::new(AtomicUsize::new(0));
    let spawn_count2 = Arc::clone(&spawn_count);
    let terminate_count = Arc::new(AtomicUsize::new(0));
    let terminate_count2 = Arc::clone(&terminate_count);

    let runtime = Builder::new_current_thread()
        .track_task_schedule_latency()
        .on_task_spawn(move |data| {
            assert_eq!(data.schedule_latency(), None);
            spawn_count2.fetch_add(1, Ordering::SeqCst);
        })
        .on_task_terminate(move |data| {
            assert_eq!(data.schedule_latency(), None);
            terminate_count2.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .unwrap();

    runtime.block_on(runtime.spawn(async {})).unwrap();
    assert_eq!(spawn_count.load(Ordering::SeqCst), 1);
    assert_eq!(terminate_count.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "schedule-latency")]
#[test]
fn task_hook_schedule_latency_disabled() {
    let runtime = Builder::new_current_thread()
        .on_before_task_poll(|data| assert_eq!(data.schedule_latency(), None))
        .build()
        .unwrap();

    runtime.block_on(runtime.spawn(async {})).unwrap();
}

#[cfg(feature = "schedule-latency")]
#[test]
fn task_hook_schedule_latency_current_thread() {
    let target = Arc::new(Mutex::new(None));
    let latencies = Arc::new(Mutex::new(Vec::new()));

    let after_latencies = Arc::new(Mutex::new(Vec::new()));

    let runtime = Builder::new_current_thread()
        .enable_metrics_schedule_latency_histogram()
        .on_before_task_poll(schedule_latency_hook(&target, &latencies))
        .on_after_task_poll(schedule_latency_hook(&target, &after_latencies))
        .build()
        .unwrap();

    let task = runtime.spawn(async {});
    *target.lock().unwrap() = Some(task.id());

    // A current-thread runtime has no background worker, so the spawned task
    // cannot be polled until `block_on` starts driving the runtime below.
    wait_for_elapsed(Duration::from_millis(50));
    runtime.block_on(task).unwrap();

    let latencies = latencies.lock().unwrap();
    assert_eq!(latencies.len(), 1);
    assert!(latencies[0] >= Duration::from_millis(25));
    assert_eq!(*latencies, *after_latencies.lock().unwrap());
}

#[cfg(feature = "schedule-latency")]
#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn task_hook_schedule_latency_multi_thread() {
    let target = Arc::new(Mutex::new(None));
    let latencies = Arc::new(Mutex::new(Vec::new()));

    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .track_task_schedule_latency()
        .on_before_task_poll(schedule_latency_hook(&target, &latencies))
        .build()
        .unwrap();

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let blocker = runtime.spawn(async move {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    started_rx.recv().unwrap();

    let task = runtime.spawn(async {});
    *target.lock().unwrap() = Some(task.id());

    wait_for_elapsed(Duration::from_millis(50));
    release_tx.send(()).unwrap();
    runtime.block_on(async {
        blocker.await.unwrap();
        task.await.unwrap();
    });

    let latencies = latencies.lock().unwrap();
    assert_eq!(latencies.len(), 1);
    assert!(latencies[0] >= Duration::from_millis(25));
}

#[cfg(feature = "schedule-latency")]
#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn task_hook_schedule_latency_multi_thread_lifo() {
    let target = Arc::new(Mutex::new(None));
    let latencies = Arc::new(Mutex::new(Vec::new()));

    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .track_task_schedule_latency()
        .on_before_task_poll(schedule_latency_hook(&target, &latencies))
        .build()
        .unwrap();

    let target2 = Arc::clone(&target);
    let parent = runtime.spawn(async move {
        let task = tokio::spawn(async {});
        *target2.lock().unwrap() = Some(task.id());
        wait_for_elapsed(Duration::from_millis(50));
        task.await.unwrap();
    });
    runtime.block_on(parent).unwrap();

    let latencies = latencies.lock().unwrap();
    assert_eq!(latencies.len(), 1);
    assert!(latencies[0] >= Duration::from_millis(25));
}

#[cfg(feature = "schedule-latency")]
fn wait_for_elapsed(duration: Duration) {
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed();
        if elapsed >= duration {
            return;
        }
        std::thread::sleep(duration - elapsed);
    }
}

#[cfg(feature = "schedule-latency")]
fn schedule_latency_hook(
    target: &Arc<Mutex<Option<tokio::task::Id>>>,
    latencies: &Arc<Mutex<Vec<Duration>>>,
) -> impl Fn(&tokio::runtime::TaskMeta<'_>) {
    let target = Arc::clone(target);
    let latencies = Arc::clone(latencies);
    move |data| {
        if Some(data.id()) == *target.lock().unwrap() {
            latencies
                .lock()
                .unwrap()
                .push(data.schedule_latency().unwrap());
        }
    }
}

fn mk_spawn_location_hook(
    event: &'static str,
    count: &Arc<AtomicUsize>,
) -> impl Fn(&tokio::runtime::TaskMeta<'_>) {
    let count = Arc::clone(count);
    move |data| {
        eprintln!("{event} ({:?}): {:?}", data.id(), data.spawned_at());
        // Assert that the spawn location is in this file.
        // Don't make assertions about line number/column here, as these
        // may change as new code is added to the test file...
        assert_eq!(
            data.spawned_at().file(),
            file!(),
            "incorrect spawn location in {event} hook",
        );
        count.fetch_add(1, Ordering::SeqCst);
    }
}
