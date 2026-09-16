#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "rt-multi-thread", not(miri)))]

use std::cell::Cell;
use std::future::pending;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, LlcAwareConfig, LlcTaskHint, Runtime};

thread_local! {
    static TEST_PARTITION: Cell<usize> = const { Cell::new(usize::MAX) };
}

fn synthetic_topology(
    partitions: usize,
    external_partition: usize,
) -> (LlcAwareConfig, Arc<AtomicUsize>) {
    assert!(partitions <= usize::BITS as usize);
    let seen = Arc::new(AtomicUsize::new(0));
    let seen_workers = seen.clone();
    let mut config = LlcAwareConfig::new(partitions, move |worker| {
        worker
            .map(|worker| {
                let partition = worker % partitions;
                TEST_PARTITION.with(|current| current.set(partition));
                seen_workers.fetch_or(1 << partition, Ordering::Release);
                partition
            })
            .or(Some(external_partition))
    });
    config.refresh_interval(1);
    (config, seen)
}

fn wait_for_partitions(seen: &AtomicUsize, partitions: usize) {
    let expected = usize::MAX >> (usize::BITS as usize - partitions);
    let deadline = Instant::now() + Duration::from_secs(5);
    while seen.load(Ordering::Acquire) != expected {
        assert!(Instant::now() < deadline, "workers did not publish their LLCs");
        std::thread::yield_now();
    }
}

fn synthetic_runtime(
    workers: usize,
    partitions: usize,
    external_partition: usize,
) -> Runtime {
    let (config, seen) = synthetic_topology(partitions, external_partition);
    let runtime = Builder::new_multi_thread()
        .worker_threads(workers)
        .llc_aware(config)
        .build()
        .unwrap();
    wait_for_partitions(&seen, partitions);
    wait_for_partition_queues(&runtime, partitions);
    runtime
}

fn wait_for_partition_queues(runtime: &Runtime, partitions: usize) {
    for partition in 0..partitions {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let task = tokio::task::Builder::new()
                .llc_partition(partition)
                .spawn_on(async { current_test_partition() }, runtime.handle())
                .unwrap();
            if runtime.block_on(task).unwrap() == partition {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "partition {partition} did not become ready"
            );
        }
    }
}

fn current_test_partition() -> usize {
    TEST_PARTITION.with(Cell::get)
}

fn block_partition(
    runtime: &Runtime,
    partition: usize,
) -> (mpsc::Sender<()>, tokio::task::JoinHandle<()>) {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let task = tokio::task::Builder::new()
        .llc_partition(partition)
        .spawn_on(
            async move {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            },
            runtime.handle(),
        )
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (release_tx, task)
}

#[test]
fn default_partition_queue_is_fifo() {
    let runtime = synthetic_runtime(1, 1, 0);
    let (release, gate) = block_partition(&runtime, 0);
    let (order_tx, order_rx) = mpsc::channel();
    let mut tasks = Vec::new();

    for value in 0..4 {
        let order_tx = order_tx.clone();
        tasks.push(
            tokio::task::Builder::new()
                .llc_partition(0)
                .spawn_on(
                    async move {
                        order_tx.send(value).unwrap();
                    },
                    runtime.handle(),
                )
                .unwrap(),
        );
    }
    drop(order_tx);
    release.send(()).unwrap();

    for expected in 0..4 {
        assert_eq!(
            order_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            expected
        );
    }
    runtime.block_on(async {
        gate.await.unwrap();
        for task in tasks {
            task.await.unwrap();
        }
    });
}

#[test]
fn local_spawn_and_remote_wake_preserve_last_polled_partition() {
    let seen = Arc::new(AtomicUsize::new(0));
    let seen_workers = seen.clone();
    let external_checks = Arc::new(AtomicUsize::new(0));
    let checked_externally = external_checks.clone();
    let mut config = LlcAwareConfig::new(2, move |worker| match worker {
        Some(worker) => {
            let partition = worker % 2;
            TEST_PARTITION.with(|current| current.set(partition));
            seen_workers.fetch_or(1 << partition, Ordering::Release);
            Some(partition)
        }
        None => {
            checked_externally.fetch_add(1, Ordering::Relaxed);
            Some(0)
        }
    });
    config.refresh_interval(1);
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .llc_aware(config)
        .build()
        .unwrap();
    wait_for_partitions(&seen, 2);
    wait_for_partition_queues(&runtime, 2);
    let (release_0, gate_0) = block_partition(&runtime, 0);
    let (partition_tx, partition_rx) = mpsc::channel();
    let (wake_tx, wake_rx) = tokio::sync::oneshot::channel();
    let (child_tx, child_rx) = mpsc::channel();

    let parent = tokio::task::Builder::new()
        .llc_partition(1)
        .spawn_on(
            async move {
                let child = tokio::spawn(async move {
                    partition_tx.send(current_test_partition()).unwrap();
                    wake_rx.await.unwrap();
                    partition_tx.send(current_test_partition()).unwrap();
                });
                child_tx.send(child).unwrap();
            },
            runtime.handle(),
        )
        .unwrap();
    let child = child_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    assert_eq!(
        partition_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap(),
        1
    );
    external_checks.store(0, Ordering::Relaxed);
    wake_tx.send(()).unwrap();
    partition_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(external_checks.load(Ordering::Relaxed), 0);
    release_0.send(()).unwrap();
    runtime.block_on(async {
        gate_0.await.unwrap();
        parent.await.unwrap();
        child.await.unwrap();
    });
}

#[test]
fn remote_spawn_uses_submitting_partition() {
    let runtime = synthetic_runtime(2, 2, 1);
    let (release, gate) = block_partition(&runtime, 0);
    let (partition_tx, partition_rx) = mpsc::channel();

    let task = runtime.spawn(async move {
        partition_tx.send(current_test_partition()).unwrap();
    });
    assert_eq!(
        partition_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap(),
        1
    );
    release.send(()).unwrap();
    runtime.block_on(async {
        gate.await.unwrap();
        task.await.unwrap();
    });
}

#[test]
fn enqueue_callback_runs_with_task_override() {
    let callback_calls = Arc::new(AtomicUsize::new(0));
    let callback_calls_inner = callback_calls.clone();
    let (mut config, seen) = synthetic_topology(2, 0);
    config.on_task_enqueue(move |_| {
        callback_calls_inner.fetch_add(1, Ordering::Relaxed);
        LlcTaskHint::new().with_partition(0)
    });
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .llc_aware(config)
        .build()
        .unwrap();
    wait_for_partitions(&seen, 2);
    wait_for_partition_queues(&runtime, 2);
    callback_calls.store(0, Ordering::Relaxed);

    let override_task = tokio::task::Builder::new()
        .llc_partition(1)
        .spawn_on(async { current_test_partition() }, runtime.handle())
        .unwrap();
    assert_eq!(runtime.block_on(override_task).unwrap(), 1);
    assert_eq!(callback_calls.load(Ordering::Relaxed), 1);

    runtime.block_on(runtime.spawn(async {})).unwrap();
    assert_eq!(callback_calls.load(Ordering::Relaxed), 2);
}

#[test]
fn enqueue_callback_can_select_global_queue() {
    let (mut config, seen) = synthetic_topology(2, 0);
    config.on_task_enqueue(|_| LlcTaskHint::new().with_global_queue());
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .llc_aware(config)
        .build()
        .unwrap();
    wait_for_partitions(&seen, 2);
    wait_for_partition_queues(&runtime, 2);
    let (release, gate) = block_partition(&runtime, 0);
    let (partition_tx, partition_rx) = mpsc::channel();

    let task = runtime.spawn(async move {
        partition_tx.send(current_test_partition()).unwrap();
    });
    assert_eq!(
        partition_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        1
    );
    release.send(()).unwrap();
    runtime.block_on(async {
        gate.await.unwrap();
        task.await.unwrap();
    });
}

#[test]
fn global_and_invalid_placements_fall_back() {
    let runtime = synthetic_runtime(2, 2, 0);
    let (release, gate) = block_partition(&runtime, 0);
    let (partition_tx, partition_rx) = mpsc::channel();

    let global = tokio::task::Builder::new()
        .global_queue()
        .spawn_on(
            {
                let partition_tx = partition_tx.clone();
                async move { partition_tx.send(current_test_partition()).unwrap() }
            },
            runtime.handle(),
        )
        .unwrap();
    let invalid = tokio::task::Builder::new()
        .llc_partition(usize::MAX)
        .spawn_on(
            async move { partition_tx.send(current_test_partition()).unwrap() },
            runtime.handle(),
        )
        .unwrap();

    for _ in 0..2 {
        assert_eq!(
            partition_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
            1
        );
    }
    release.send(()).unwrap();
    runtime.block_on(async {
        gate.await.unwrap();
        global.await.unwrap();
        invalid.await.unwrap();
    });
}

#[test]
fn partition_without_a_worker_falls_back_to_global_queue() {
    let seen = Arc::new(AtomicUsize::new(0));
    let seen_workers = seen.clone();
    let config = LlcAwareConfig::new(2, move |worker| {
        if let Some(worker) = worker {
            TEST_PARTITION.with(|current| current.set(0));
            seen_workers.fetch_or(1 << worker, Ordering::Release);
        }
        Some(0)
    });
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .llc_aware(config)
        .build()
        .unwrap();
    wait_for_partitions(&seen, 2);
    let (release, gate) = block_partition(&runtime, 0);
    let (partition_tx, partition_rx) = mpsc::channel();

    let task = tokio::task::Builder::new()
        .llc_partition(1)
        .spawn_on(
            async move { partition_tx.send(current_test_partition()).unwrap() },
            runtime.handle(),
        )
        .unwrap();
    assert_eq!(
        partition_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap(),
        0
    );
    release.send(()).unwrap();
    runtime.block_on(async {
        gate.await.unwrap();
        task.await.unwrap();
    });
}

#[test]
fn cross_llc_search_prefers_the_same_numa_node() {
    let (mut config, seen) = synthetic_topology(3, 0);
    config.numa_node_map([0, 0, 1]);
    let runtime = Builder::new_multi_thread()
        .worker_threads(3)
        .llc_aware(config)
        .build()
        .unwrap();
    wait_for_partitions(&seen, 3);
    wait_for_partition_queues(&runtime, 3);
    let (release_0, gate_0) = block_partition(&runtime, 0);
    let (release_1, gate_1) = block_partition(&runtime, 1);
    let (release_2, gate_2) = block_partition(&runtime, 2);
    let (order_tx, order_rx) = mpsc::channel();

    let cross_numa = tokio::task::Builder::new()
        .llc_partition(2)
        .spawn_on(
            {
                let order_tx = order_tx.clone();
                async move { order_tx.send("cross-numa").unwrap() }
            },
            runtime.handle(),
        )
        .unwrap();
    let same_numa = tokio::task::Builder::new()
        .llc_partition(1)
        .spawn_on(
            async move { order_tx.send("same-numa").unwrap() },
            runtime.handle(),
        )
        .unwrap();

    release_0.send(()).unwrap();
    assert_eq!(
        order_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "same-numa"
    );
    assert_eq!(
        order_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "cross-numa"
    );
    release_1.send(()).unwrap();
    release_2.send(()).unwrap();
    runtime.block_on(async {
        gate_0.await.unwrap();
        gate_1.await.unwrap();
        gate_2.await.unwrap();
        same_numa.await.unwrap();
        cross_numa.await.unwrap();
    });
}

#[test]
fn cross_llc_stealing_neither_loses_nor_duplicates_tasks() {
    const TASKS: usize = 64;

    let runtime = synthetic_runtime(2, 2, 0);
    let (release, gate) = block_partition(&runtime, 1);
    let executions = (0..TASKS)
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let executions: Arc<[AtomicUsize]> = executions.into();
    let (done_tx, done_rx) = mpsc::channel();
    let mut tasks = Vec::with_capacity(TASKS);

    for index in 0..TASKS {
        let executions = executions.clone();
        let done_tx = done_tx.clone();
        tasks.push(
            tokio::task::Builder::new()
                .llc_partition(1)
                .spawn_on(
                    async move {
                        executions[index].fetch_add(1, Ordering::Relaxed);
                        done_tx.send(()).unwrap();
                    },
                    runtime.handle(),
                )
                .unwrap(),
        );
    }
    drop(done_tx);

    for _ in 0..TASKS {
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert!(
        executions
            .iter()
            .all(|executions| executions.load(Ordering::Relaxed) == 1)
    );
    release.send(()).unwrap();
    runtime.block_on(async {
        gate.await.unwrap();
        for task in tasks {
            task.await.unwrap();
        }
    });
}

#[test]
fn shutdown_drops_tasks_from_llc_queues() {
    struct CountDrop(Arc<AtomicUsize>);

    impl Drop for CountDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    const TASKS: usize = 32;
    let dropped = Arc::new(AtomicUsize::new(0));
    let runtime = synthetic_runtime(2, 2, 0);
    let mut handles = Vec::with_capacity(TASKS);

    for index in 0..TASKS {
        let guard = CountDrop(dropped.clone());
        handles.push(
            tokio::task::Builder::new()
                .llc_partition(index % 2)
                .spawn_on(
                    async move {
                        let _guard = guard;
                        pending::<()>().await;
                    },
                    runtime.handle(),
                )
                .unwrap(),
        );
    }

    drop(handles);
    drop(runtime);
    assert_eq!(dropped.load(Ordering::Relaxed), TASKS);
}

#[test]
fn default_remote_spawn_checks_the_submitting_llc() {
    let external_checks = Arc::new(AtomicUsize::new(0));
    let config = LlcAwareConfig::new(1, {
        let external_checks = external_checks.clone();
        move |worker| {
            if worker.is_none() {
                external_checks.fetch_add(1, Ordering::Relaxed);
            }
            Some(0)
        }
    });
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .llc_aware(config)
        .build()
        .unwrap();

    runtime.block_on(runtime.spawn(async {})).unwrap();
    assert!(external_checks.load(Ordering::Relaxed) > 0);
}

#[test]
fn zero_partitions_uses_fallback() {
    assert_eq!(LlcAwareConfig::new(0, |_| None).partition_count(), 1);
}

#[test]
fn zero_refresh_interval_uses_fallback() {
    LlcAwareConfig::new(1, |_| Some(0)).refresh_interval(0);
}

#[test]
fn zero_scan_limit_uses_fallback() {
    LlcAwareConfig::new(1, |_| Some(0)).cross_llc_scan_limit(0);
}

#[test]
fn zero_numa_scan_limit_uses_fallback() {
    LlcAwareConfig::new(1, |_| Some(0)).cross_numa_scan_limit(0);
}

#[test]
fn zero_cross_llc_batch_uses_fallback() {
    LlcAwareConfig::new(1, |_| Some(0)).cross_llc_steal_batch(0);
}

#[test]
fn zero_cross_numa_batch_uses_fallback() {
    LlcAwareConfig::new(1, |_| Some(0)).cross_numa_steal_batch(0);
}

#[test]
fn invalid_numa_map_uses_fallback() {
    let mut config = LlcAwareConfig::new(2, |_| Some(0));
    config.numa_node_map([0]);
    assert_eq!(config.numa_node_count(), 1);
}

#[test]
fn numa_node_ids_are_normalized() {
    let mut config = LlcAwareConfig::new(3, |_| Some(0));
    config.numa_node_map([4, 4, 9]);
    assert_eq!(config.numa_node_count(), 2);
}

#[test]
#[cfg(target_os = "linux")]
fn automatic_topology_is_best_effort() {
    let mut builder = Builder::new_multi_thread();
    builder.worker_threads(1).enable_llc_aware();
    let runtime = builder.build().unwrap();
    runtime.block_on(runtime.spawn(async {})).unwrap();
}

#[test]
fn too_few_workers_falls_back() {
    let partition_checks = Arc::new(AtomicUsize::new(0));
    let config = LlcAwareConfig::new(2, {
        let partition_checks = partition_checks.clone();
        move |_| {
            partition_checks.fetch_add(1, Ordering::Relaxed);
            Some(0)
        }
    });
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .llc_aware(config)
        .build()
        .unwrap();

    runtime.block_on(runtime.spawn(async {})).unwrap();
    assert_eq!(partition_checks.load(Ordering::Relaxed), 0);
}
