#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, target_has_atomic = "64"))]

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

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
        .on_task_spawn(move |data, _parent| {
            assert!(data.data::<usize>().is_none());
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

#[test]
fn spawn_blocking_task_hooks_are_balanced() {
    let (spawned_tx, spawned_rx) = std::sync::mpsc::channel();
    let (terminated_tx, terminated_rx) = std::sync::mpsc::channel();

    let runtime = Builder::new_current_thread()
        .on_task_spawn(move |meta, parent| {
            assert!(parent.is_none());
            spawned_tx.send(meta.id()).unwrap();
        })
        .on_task_terminate(move |meta| {
            terminated_tx.send(meta.id()).unwrap();
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::task::spawn_blocking(|| {}).await.unwrap();
    });

    let spawned = spawned_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("spawn_blocking task did not fire spawn hook");
    let terminated = terminated_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("spawn_blocking task did not fire terminate hook");

    assert_eq!(spawned, terminated);
    assert!(matches!(
        spawned_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        terminated_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}

#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn internal_runtime_blocking_tasks_do_not_fire_task_hooks() {
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let spawned2 = Arc::clone(&spawned);
    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .on_task_spawn(move |meta, _parent| {
            spawned2.lock().unwrap().push(meta.spawned_at().file());
        })
        .on_task_terminate(move |meta| {
            terminated2.lock().unwrap().push(meta.spawned_at().file());
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::spawn(async {
            tokio::task::block_in_place(|| {});
        })
        .await
        .unwrap();
    });

    runtime.shutdown_timeout(Duration::from_secs(5));

    let spawned = spawned.lock().unwrap();
    assert!(
        !spawned.iter().any(is_multi_thread_worker_file),
        "internal worker task fired spawn hook: {spawned:?}"
    );

    let terminated = terminated.lock().unwrap();
    assert!(
        !terminated.iter().any(is_multi_thread_worker_file),
        "internal worker task fired terminate hook: {terminated:?}"
    );
}

fn is_multi_thread_worker_file(file: &&'static str) -> bool {
    file.ends_with("runtime/scheduler/multi_thread/worker.rs")
        || file.ends_with(r"runtime\scheduler\multi_thread\worker.rs")
}

#[test]
fn spawn_blocking_after_shutdown_terminate_hook_can_reenter_pool() {
    let handle: Arc<Mutex<Option<tokio::runtime::Handle>>> = Arc::new(Mutex::new(None));
    let hook_handle = Arc::clone(&handle);
    let terminated = Arc::new(AtomicUsize::new(0));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_current_thread()
        .on_task_terminate(move |_meta| {
            if terminated2.fetch_add(1, Ordering::SeqCst) == 0 {
                let handle = hook_handle.lock().unwrap().clone().unwrap();
                drop(handle.spawn_blocking(|| {}));
            }
        })
        .build()
        .unwrap();

    let runtime_handle = runtime.handle().clone();
    *handle.lock().unwrap() = Some(runtime_handle.clone());
    runtime.shutdown_timeout(Duration::from_secs(5));

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(runtime_handle.spawn_blocking(|| {}));
        done_tx.send(()).unwrap();
    });

    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("terminate hook deadlocked while re-entering the blocking pool");

    assert_eq!(terminated.load(Ordering::SeqCst), 2);
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
        .on_before_task_poll(mk_poll_location_hook(
            "(current_thread) on_before_task_poll",
            &poll_starts,
        ))
        .on_after_task_poll(mk_poll_location_hook(
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
        .on_before_task_poll(mk_poll_location_hook(
            "(multi_thread) on_before_task_poll",
            &poll_starts,
        ))
        .on_after_task_poll(mk_poll_location_hook(
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

#[derive(Debug)]
struct PollState {
    before: usize,
    after: usize,
}

#[test]
fn task_data_mutates_across_current_thread_hooks() {
    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(|meta, parent| {
            assert!(parent.is_none());
            assert!(!meta.clear_data());
            meta.set_data(PollState {
                before: 0,
                after: 0,
            });
        })
        .on_before_task_poll(|meta| {
            meta.data_mut::<PollState>().unwrap().before += 1;
        })
        .on_after_task_poll(|meta| {
            if let Some(data) = meta.data_mut::<PollState>() {
                data.after += 1;
            }
        })
        .on_task_terminate(move |meta| {
            let data = meta.take_data::<PollState>().unwrap();
            assert!(meta.data::<PollState>().is_none());
            terminated2.lock().unwrap().push((data.before, data.after));
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::spawn(async {
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    });

    let terminated = terminated.lock().unwrap();
    assert_eq!(terminated.len(), 1);
    let (before, after) = terminated[0];
    assert!(before > 1);
    assert_eq!(before, after);
}

#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn task_data_mutates_across_multi_thread_hooks() {
    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .on_task_spawn(|meta, _parent| {
            meta.set_data(PollState {
                before: 0,
                after: 0,
            });
        })
        .on_before_task_poll(|meta| {
            meta.data_mut::<PollState>().unwrap().before += 1;
        })
        .on_after_task_poll(|meta| {
            if let Some(data) = meta.data_mut::<PollState>() {
                data.after += 1;
            }
        })
        .on_task_terminate(move |meta| {
            let data = meta.take_data::<PollState>().unwrap();
            terminated2.lock().unwrap().push((data.before, data.after));
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::spawn(async {
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    });

    runtime.shutdown_timeout(std::time::Duration::from_secs(60));

    let terminated = terminated.lock().unwrap();
    assert_eq!(terminated.len(), 1);
    let (before, after) = terminated[0];
    assert!(before > 1);
    assert_eq!(before, after);
}

#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn poll_hook_data_is_not_terminated_during_multi_thread_shutdown() {
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let entered2 = Arc::clone(&entered);
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release2 = Arc::clone(&release);
    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .on_task_spawn(|meta, _parent| {
            meta.set_data(Vec::<&'static str>::new());
        })
        .on_before_task_poll(move |meta| {
            let (lock, cvar) = &*entered2;
            *lock.lock().unwrap() = true;
            cvar.notify_one();

            let (lock, cvar) = &*release2;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cvar.wait(released).unwrap();
            }

            meta.data_mut::<Vec<&'static str>>()
                .unwrap()
                .push("before_done");
        })
        .on_task_terminate(move |meta| {
            let data = meta.take_data::<Vec<&'static str>>().unwrap();
            terminated2.lock().unwrap().push(*data);
        })
        .build()
        .unwrap();

    drop(runtime.spawn(std::future::pending::<()>()));

    let (lock, cvar) = &*entered;
    let entered_guard = lock.lock().unwrap();
    let (entered_guard, wait_result) = cvar
        .wait_timeout_while(entered_guard, Duration::from_secs(5), |entered| !*entered)
        .unwrap();
    assert!(*entered_guard);
    assert!(!wait_result.timed_out());

    let shutdown = std::thread::spawn(move || {
        runtime.shutdown_timeout(Duration::from_secs(5));
    });

    std::thread::sleep(Duration::from_millis(50));

    let (lock, cvar) = &*release;
    *lock.lock().unwrap() = true;
    cvar.notify_one();

    shutdown.join().unwrap();

    assert_eq!(*terminated.lock().unwrap(), vec![vec!["before_done"]]);
}

#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn abort_during_before_poll_hook_does_not_poll_future() {
    struct CountPolls {
        polls: Arc<AtomicUsize>,
    }

    impl Future for CountPolls {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Pending
        }
    }

    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let entered2 = Arc::clone(&entered);
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release2 = Arc::clone(&release);
    let polls = Arc::new(AtomicUsize::new(0));

    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .on_before_task_poll(move |_meta| {
            let (lock, cvar) = &*entered2;
            *lock.lock().unwrap() = true;
            cvar.notify_one();

            let (lock, cvar) = &*release2;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cvar.wait(released).unwrap();
            }
        })
        .build()
        .unwrap();

    let task = runtime.spawn(CountPolls {
        polls: Arc::clone(&polls),
    });

    let (lock, cvar) = &*entered;
    let entered_guard = lock.lock().unwrap();
    let (entered_guard, wait_result) = cvar
        .wait_timeout_while(entered_guard, Duration::from_secs(5), |entered| !*entered)
        .unwrap();
    assert!(*entered_guard);
    assert!(!wait_result.timed_out());

    task.abort();

    let (lock, cvar) = &*release;
    *lock.lock().unwrap() = true;
    cvar.notify_one();

    let err = runtime.block_on(task).unwrap_err();
    assert!(err.is_cancelled());
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Lineage {
    depth: usize,
}

#[test]
fn spawn_hook_can_inherit_parent_task_data() {
    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(|meta, parent| {
            let depth = match parent {
                Some(parent) => parent
                    .data::<Lineage>()
                    .map_or(0, |parent| parent.depth + 1),
                None => 0,
            };

            meta.set_data(Lineage { depth });
        })
        .on_task_terminate(move |meta| {
            let data = meta.take_data::<Lineage>().unwrap();
            terminated2.lock().unwrap().push(data.depth);
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::spawn(async {
            tokio::spawn(async {}).await.unwrap();
        })
        .await
        .unwrap();
    });

    let mut terminated = terminated.lock().unwrap().clone();
    terminated.sort_unstable();
    assert_eq!(terminated, vec![0, 1]);
}

#[test]
fn spawn_hook_can_inherit_parent_task_data_from_future_drop() {
    struct SpawnOnDrop;

    impl Future for SpawnOnDrop {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            Poll::Pending
        }
    }

    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            tokio::spawn(async {});
        }
    }

    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(|meta, parent| {
            let depth = match parent {
                Some(parent) => parent
                    .data::<Lineage>()
                    .map_or(0, |parent| parent.depth + 1),
                None => 0,
            };

            meta.set_data(Lineage { depth });
        })
        .on_task_terminate(move |meta| {
            let data = meta.take_data::<Lineage>().unwrap();
            terminated2.lock().unwrap().push(data.depth);
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        let task = tokio::spawn(SpawnOnDrop);
        tokio::task::yield_now().await;

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());

        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
    });

    let mut terminated = terminated.lock().unwrap().clone();
    terminated.sort_unstable();
    assert_eq!(terminated, vec![0, 1]);
}

#[test]
fn spawn_hook_runs_before_terminate_when_current_thread_runtime_is_closed() {
    struct ClosedSpawnData;

    let events = Arc::new(Mutex::new(Vec::new()));
    let events2 = Arc::clone(&events);
    let events3 = Arc::clone(&events);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(move |meta, _parent| {
            meta.set_data(ClosedSpawnData);
            events2.lock().unwrap().push("spawn");
        })
        .on_task_terminate(move |meta| {
            if meta.take_data::<ClosedSpawnData>().is_some() {
                events3.lock().unwrap().push("terminate_with_data");
            }
        })
        .build()
        .unwrap();

    let handle = runtime.handle().clone();
    drop(runtime);

    drop(handle.spawn(async {}));

    assert_eq!(*events.lock().unwrap(), ["spawn", "terminate_with_data"]);
}

#[cfg_attr(
    target_os = "wasi",
    ignore = "WASI does not support multi-threaded runtime"
)]
#[test]
fn spawn_hook_runs_before_terminate_when_multi_thread_runtime_is_closed() {
    struct ClosedSpawnData;

    let events = Arc::new(Mutex::new(Vec::new()));
    let events2 = Arc::clone(&events);
    let events3 = Arc::clone(&events);

    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .on_task_spawn(move |meta, _parent| {
            meta.set_data(ClosedSpawnData);
            events2.lock().unwrap().push("spawn");
        })
        .on_task_terminate(move |meta| {
            if meta.take_data::<ClosedSpawnData>().is_some() {
                events3.lock().unwrap().push("terminate_with_data");
            }
        })
        .build()
        .unwrap();

    let handle = runtime.handle().clone();
    drop(runtime);

    drop(handle.spawn(async {}));

    assert_eq!(*events.lock().unwrap(), ["spawn", "terminate_with_data"]);
}

#[cfg(feature = "tracing")]
#[test]
fn task_builder_data_is_visible_to_hooks() {
    let terminated = Arc::new(Mutex::new(Vec::new()));
    let terminated2 = Arc::clone(&terminated);

    let runtime = Builder::new_current_thread()
        .on_task_spawn(|meta, _parent| {
            let value = meta.data_mut::<usize>().unwrap();
            *value += 1;
        })
        .on_task_terminate(move |meta| {
            let value = meta.take_data::<usize>().unwrap();
            terminated2.lock().unwrap().push(*value);
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::task::Builder::new()
            .data(41usize)
            .spawn(async {})
            .unwrap()
            .await
            .unwrap();
    });

    assert_eq!(*terminated.lock().unwrap(), vec![42]);
}

#[cfg(feature = "tracing")]
#[test]
fn task_builder_data_is_not_dropped_for_spawn_blocking() {
    let (terminated_tx, terminated_rx) = std::sync::mpsc::channel();

    let runtime = Builder::new_current_thread()
        .on_task_terminate(move |meta| {
            if let Some(value) = meta.take_data::<usize>() {
                terminated_tx.send(*value).unwrap();
            }
        })
        .build()
        .unwrap();

    runtime.block_on(async {
        tokio::task::Builder::new()
            .data(7usize)
            .spawn_blocking(|| {})
            .unwrap()
            .await
            .unwrap();
    });

    let terminated = terminated_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("spawn_blocking task terminate hook did not receive task data");
    assert_eq!(terminated, 7);
}

fn mk_spawn_location_hook(
    event: &'static str,
    count: &Arc<AtomicUsize>,
) -> impl Fn(&mut tokio::runtime::TaskMeta<'_>, Option<tokio::runtime::TaskMetaRef<'_>>) {
    let count = Arc::clone(count);
    move |data, _parent| {
        assert_spawn_location(event, count.as_ref(), data);
    }
}

fn mk_poll_location_hook(
    event: &'static str,
    count: &Arc<AtomicUsize>,
) -> impl Fn(&mut tokio::runtime::TaskMeta<'_>) {
    let count = Arc::clone(count);
    move |data| {
        assert_spawn_location(event, count.as_ref(), data);
    }
}

fn assert_spawn_location(
    event: &'static str,
    count: &AtomicUsize,
    data: &tokio::runtime::TaskMeta<'_>,
) {
    eprintln!("{event} ({:?}): {:?}", data.id(), data.spawned_at());
    assert_eq!(
        data.spawned_at().file(),
        file!(),
        "incorrect spawn location in {event} hook",
    );
    count.fetch_add(1, Ordering::SeqCst);
}
