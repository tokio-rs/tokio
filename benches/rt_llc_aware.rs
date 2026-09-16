//! A/B benchmarks for the LLC-aware multi-thread scheduler.
//!
//! Run with:
//! `RUSTFLAGS="--cfg tokio_unstable" cargo bench --manifest-path benches/Cargo.toml --bench rt_llc_aware`
//!
//! `TOKIO_LLC_BENCH_WORKERS` overrides the worker count. The default is the
//! number of logical CPUs in this process's allowed CPU set.
//! `TOKIO_LLC_BENCH_TASKS` overrides the number of tasks in the spawn and wake
//! benchmarks. The default is 256 tasks per allowed logical CPU, or 4,096,
//! whichever is greater.
//! `TOKIO_LLC_BENCH_TASK_WORK` controls the number of CPU-work iterations each
//! short task performs in one poll. The default is zero to isolate scheduler
//! overhead; values such as 64, 4,096, and 65,536 exercise larger tasks.
//! `TOKIO_LLC_BENCH_PRODUCERS` overrides the number of external producer
//! threads. The default is up to four pinned producers per LLC, capped by the
//! smallest allowed logical-CPU count of any LLC. The producer count must be a
//! multiple of the discovered LLC count. Producer threads persist across all
//! measured iterations and begin each batch at a barrier.
//! Workers and producers are pinned across the discovered LLCs so placement
//! remains consistent between enabled and disabled runs.
//!
//! `TOKIO_LLC_BENCH_CACHE_BYTES` overrides the per-task working-set size in
//! the cache-affine wake benchmark. The default is 2 MiB per task, with two
//! tasks per LLC. Each task first-touches its allocation on its target LLC.
//! `TOKIO_LLC_BENCH_CACHE_TASKS_PER_LLC` overrides the number of cache-heavy
//! tasks assigned to each LLC.

#[cfg(all(tokio_unstable, target_os = "linux"))]
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
#[cfg(all(tokio_unstable, target_os = "linux"))]
use std::collections::BTreeMap;
#[cfg(all(tokio_unstable, target_os = "linux"))]
use std::future::poll_fn;
#[cfg(all(tokio_unstable, target_os = "linux"))]
use std::io;
#[cfg(all(tokio_unstable, target_os = "linux"))]
use std::sync::{
    atomic::{AtomicUsize, Ordering::Relaxed},
    mpsc, Arc, Barrier,
};
#[cfg(all(tokio_unstable, target_os = "linux"))]
use std::task::{Poll, Waker};
#[cfg(all(tokio_unstable, target_os = "linux"))]
use std::time::{Duration, Instant};
#[cfg(all(tokio_unstable, target_os = "linux"))]
use tokio::runtime::{Builder, LlcAwareConfig, Runtime};

#[cfg(all(tokio_unstable, target_os = "linux"))]
const DEFAULT_TASKS: usize = 4_096;

#[cfg(all(tokio_unstable, target_os = "linux"))]
const DEFAULT_TASKS_PER_CPU: usize = 256;

#[cfg(all(tokio_unstable, target_os = "linux"))]
const DEFAULT_PRODUCERS_PER_PARTITION: usize = 4;

#[cfg(all(tokio_unstable, target_os = "linux"))]
const CACHE_BYTES_PER_TASK: usize = 2 * 1024 * 1024;

#[cfg(all(tokio_unstable, target_os = "linux"))]
const CACHE_TASKS_PER_PARTITION: usize = 2;

#[cfg(all(tokio_unstable, target_os = "linux"))]
const CACHE_ROUNDS: usize = 4;

#[cfg(all(tokio_unstable, target_os = "linux"))]
#[derive(Clone, Copy)]
enum Mode {
    Disabled,
    Enabled,
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Enabled => "enabled",
        }
    }
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
#[derive(Clone, Copy)]
enum LocalWorkload {
    Balanced,
    SingleWorker,
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
impl LocalWorkload {
    fn name(self) -> &'static str {
        match self {
            Self::Balanced => "local_spawn",
            Self::SingleWorker => "work_stealing",
        }
    }
}

/// Measures worker-local spawning with one spawning task per LLC.
#[cfg(all(tokio_unstable, target_os = "linux"))]
fn local_spawn(c: &mut Criterion) {
    local_workload(c, LocalWorkload::Balanced);
}

/// Measures an imbalanced worker-local batch which must be spilled or stolen
/// for the full runtime to participate.
#[cfg(all(tokio_unstable, target_os = "linux"))]
fn work_stealing(c: &mut Criterion) {
    local_workload(c, LocalWorkload::SingleWorker);
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn local_workload(c: &mut Criterion, workload: LocalWorkload) {
    let (workers, topology) = benchmark_topology();
    let partitions = topology.partition_count();
    let worker_cpus = benchmark_worker_cpus(workers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let tasks = benchmark_tasks();
    let task_work = benchmark_task_work();
    let spawners = match workload {
        LocalWorkload::Balanced => partitions,
        LocalWorkload::SingleWorker => 1,
    };
    let mut group = c.benchmark_group(format!(
        "llc_aware/{}/{workers}_workers/{tasks}_tasks/{task_work}_work",
        workload.name()
    ));
    group.throughput(Throughput::Elements(tasks as u64));

    for mode in [Mode::Disabled, Mode::Enabled] {
        let runtime = pinned_runtime(mode, workers, topology.clone(), worker_cpus.clone());
        group.bench_with_input(BenchmarkId::from_parameter(mode.name()), &mode, |b, _| {
            b.iter_custom(|iterations| {
                let mut elapsed = Duration::ZERO;

                for _ in 0..iterations {
                    let (ready_tx, ready_rx) = mpsc::channel();
                    let mut start_txs = Vec::with_capacity(spawners);
                    let mut spawner_handles = Vec::with_capacity(spawners);

                    for spawner in 0..spawners {
                        let ready_tx = ready_tx.clone();
                        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
                        start_txs.push(start_tx);
                        let task_range = producer_task_range(tasks, spawners, spawner);
                        let partition = spawner % partitions;
                        spawner_handles.push(
                            tokio::task::Builder::new()
                                .llc_partition(partition)
                                .spawn_on(
                                    async move {
                                        ready_tx.send(()).unwrap();
                                        start_rx.await.unwrap();
                                        let handles = task_range
                                            .map(|task| {
                                                tokio::spawn(async move {
                                                    run_task_work(task_work, task);
                                                })
                                            })
                                            .collect::<Vec<_>>();
                                        for handle in handles {
                                            handle.await.unwrap();
                                        }
                                    },
                                    runtime.handle(),
                                )
                                .unwrap(),
                        );
                    }
                    drop(ready_tx);
                    for _ in 0..spawners {
                        ready_rx.recv().unwrap();
                    }

                    let start = Instant::now();
                    for start_tx in start_txs {
                        start_tx.send(()).unwrap();
                    }
                    runtime.block_on(async {
                        for handle in spawner_handles {
                            handle.await.unwrap();
                        }
                    });
                    elapsed += start.elapsed();
                }

                elapsed
            });
        });
    }

    group.finish();
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn remote_spawn(c: &mut Criterion) {
    let (workers, topology) = benchmark_topology();
    let partitions = topology.partition_count();
    let worker_cpus = benchmark_worker_cpus(workers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let producers = benchmark_producers(partitions);
    let producer_cpus = benchmark_worker_cpus(producers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let tasks = benchmark_tasks();
    let task_work = benchmark_task_work();
    let mut group = c.benchmark_group(format!(
        "llc_aware/remote_spawn/{workers}_workers/{producers}_producers/{tasks}_tasks/{task_work}_work"
    ));
    group.throughput(Throughput::Elements(tasks as u64));

    for mode in [Mode::Disabled, Mode::Enabled] {
        let runtime = pinned_runtime(mode, workers, topology.clone(), worker_cpus.clone());
        group.bench_with_input(BenchmarkId::from_parameter(mode.name()), &mode, |b, _| {
            let barrier = Arc::new(Barrier::new(producers + 1));
            let (handles_tx, handles_rx) = mpsc::channel();
            let mut command_txs = Vec::with_capacity(producers);
            let mut producer_threads = Vec::with_capacity(producers);

            for producer in 0..producers {
                let (command_tx, command_rx) = mpsc::channel::<()>();
                command_txs.push(command_tx);
                let barrier = barrier.clone();
                let handles_tx = handles_tx.clone();
                let runtime = runtime.handle().clone();
                let cpu = producer_cpus[producer];
                let task_range = producer_task_range(tasks, producers, producer);
                producer_threads.push(std::thread::spawn(move || {
                    pin_current_thread(cpu);
                    while command_rx.recv().is_ok() {
                        barrier.wait();
                        let handles = task_range
                            .clone()
                            .map(|task| {
                                runtime.spawn(async move {
                                    run_task_work(task_work, task);
                                })
                            })
                            .collect::<Vec<_>>();
                        handles_tx.send(handles).unwrap();
                    }
                }));
            }
            drop(handles_tx);

            b.iter_custom(|iterations| {
                let mut elapsed = Duration::ZERO;

                for _ in 0..iterations {
                    for command_tx in &command_txs {
                        command_tx.send(()).unwrap();
                    }

                    let start = Instant::now();
                    barrier.wait();
                    let mut handles = Vec::with_capacity(tasks);
                    for _ in 0..producers {
                        let mut batch = handles_rx.recv().unwrap();
                        handles.append(&mut batch);
                    }
                    runtime.block_on(async {
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                    elapsed += start.elapsed();
                }

                elapsed
            });

            drop(command_txs);
            for producer in producer_threads {
                producer.join().unwrap();
            }
        });
    }

    group.finish();
}

/// Measures explicit per-task placement through `task::Builder`.
#[cfg(all(tokio_unstable, target_os = "linux"))]
fn hinted_spawn(c: &mut Criterion) {
    let (workers, topology) = benchmark_topology();
    let partitions = topology.partition_count();
    let worker_cpus = benchmark_worker_cpus(workers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let producers = benchmark_producers(partitions);
    let producer_cpus = benchmark_worker_cpus(producers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let tasks = benchmark_tasks();
    let task_work = benchmark_task_work();
    let mut group = c.benchmark_group(format!(
        "llc_aware/hinted_spawn/{workers}_workers/{producers}_producers/{tasks}_tasks/{task_work}_work"
    ));
    group.throughput(Throughput::Elements(tasks as u64));

    for mode in [Mode::Disabled, Mode::Enabled] {
        let runtime = pinned_runtime(mode, workers, topology.clone(), worker_cpus.clone());
        group.bench_with_input(BenchmarkId::from_parameter(mode.name()), &mode, |b, _| {
            let barrier = Arc::new(Barrier::new(producers + 1));
            let (handles_tx, handles_rx) = mpsc::channel();
            let mut command_txs = Vec::with_capacity(producers);
            let mut producer_threads = Vec::with_capacity(producers);

            for producer in 0..producers {
                let (command_tx, command_rx) = mpsc::channel::<()>();
                command_txs.push(command_tx);
                let barrier = barrier.clone();
                let handles_tx = handles_tx.clone();
                let runtime = runtime.handle().clone();
                let cpu = producer_cpus[producer];
                let partition = producer % partitions;
                let task_range = producer_task_range(tasks, producers, producer);
                producer_threads.push(std::thread::spawn(move || {
                    pin_current_thread(cpu);
                    while command_rx.recv().is_ok() {
                        barrier.wait();
                        let handles = task_range
                            .clone()
                            .map(|task| {
                                tokio::task::Builder::new()
                                    .llc_partition(partition)
                                    .spawn_on(
                                        async move {
                                            run_task_work(task_work, task);
                                        },
                                        &runtime,
                                    )
                                    .unwrap()
                            })
                            .collect::<Vec<_>>();
                        handles_tx.send(handles).unwrap();
                    }
                }));
            }
            drop(handles_tx);

            b.iter_custom(|iterations| {
                let mut elapsed = Duration::ZERO;

                for _ in 0..iterations {
                    for command_tx in &command_txs {
                        command_tx.send(()).unwrap();
                    }

                    let start = Instant::now();
                    barrier.wait();
                    let mut handles = Vec::with_capacity(tasks);
                    for _ in 0..producers {
                        let mut batch = handles_rx.recv().unwrap();
                        handles.append(&mut batch);
                    }
                    runtime.block_on(async {
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                    elapsed += start.elapsed();
                }

                elapsed
            });

            drop(command_txs);
            for producer in producer_threads {
                producer.join().unwrap();
            }
        });
    }

    group.finish();
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
#[derive(Clone, Copy)]
enum WakeLocality {
    SameLlc,
    CrossLlc,
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
impl WakeLocality {
    fn name(self) -> &'static str {
        match self {
            Self::SameLlc => "same_llc_wake",
            Self::CrossLlc => "cross_llc_wake",
        }
    }
}

/// Measures remote wakeups of tasks which have already run on a worker.
#[cfg(all(tokio_unstable, target_os = "linux"))]
fn same_llc_wake(c: &mut Criterion) {
    remote_wake(c, WakeLocality::SameLlc);
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn cross_llc_wake(c: &mut Criterion) {
    remote_wake(c, WakeLocality::CrossLlc);
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn remote_wake(c: &mut Criterion, locality: WakeLocality) {
    let (workers, topology) = benchmark_topology();
    let partitions = topology.partition_count();
    let worker_cpus = benchmark_worker_cpus(workers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let producers = benchmark_producers(partitions);
    let producers_per_partition = producers / partitions;
    let producer_cpus = benchmark_worker_cpus(producers, partitions)
        .expect("the LLC benchmark requires Linux sysfs topology");
    let tasks = benchmark_tasks();
    let task_work = benchmark_task_work();
    let mut group = c.benchmark_group(format!(
        "llc_aware/{}/{workers}_workers/{producers}_producers/{tasks}_tasks/{task_work}_work",
        locality.name()
    ));
    group.throughput(Throughput::Elements(tasks as u64));

    for mode in [Mode::Disabled, Mode::Enabled] {
        let runtime = pinned_runtime(mode, workers, topology.clone(), worker_cpus.clone());
        group.bench_with_input(BenchmarkId::from_parameter(mode.name()), &mode, |b, _| {
            let barrier = Arc::new(Barrier::new(producers + 1));
            let mut command_txs = Vec::with_capacity(producers);
            let mut producer_threads = Vec::with_capacity(producers);

            for producer in 0..producers {
                let (command_tx, command_rx) =
                    mpsc::channel::<Vec<tokio::sync::oneshot::Sender<()>>>();
                command_txs.push(command_tx);
                let barrier = barrier.clone();
                let cpu = producer_cpus[producer];
                producer_threads.push(std::thread::spawn(move || {
                    pin_current_thread(cpu);
                    while let Ok(wakes) = command_rx.recv() {
                        barrier.wait();
                        for wake in wakes {
                            wake.send(()).unwrap();
                        }
                    }
                }));
            }

            b.iter_custom(|iterations| {
                let mut elapsed = Duration::ZERO;

                for _ in 0..iterations {
                    let (ready_tx, ready_rx) = mpsc::channel();
                    let mut wake_batches = (0..producers)
                        .map(|_| Vec::with_capacity(tasks / producers + 1))
                        .collect::<Vec<_>>();
                    let mut handles = Vec::with_capacity(tasks);

                    for task in 0..tasks {
                        let ready_tx = ready_tx.clone();
                        let (wake_tx, wake_rx) = tokio::sync::oneshot::channel();
                        let partition = task % partitions;
                        let producer_lane = (task / partitions) % producers_per_partition;
                        let producer_partition = match locality {
                            WakeLocality::SameLlc => partition,
                            WakeLocality::CrossLlc => (partition + 1) % partitions,
                        };
                        let producer = producer_lane * partitions + producer_partition;
                        wake_batches[producer].push(wake_tx);
                        handles.push(
                            tokio::task::Builder::new()
                                .llc_partition(partition)
                                .spawn_on(
                                    async move {
                                        ready_tx.send(()).unwrap();
                                        wake_rx.await.unwrap();
                                        run_task_work(task_work, task);
                                    },
                                    runtime.handle(),
                                )
                                .unwrap(),
                        );
                    }
                    drop(ready_tx);
                    for _ in 0..tasks {
                        ready_rx.recv().unwrap();
                    }

                    for (command_tx, wakes) in command_txs.iter().zip(wake_batches) {
                        command_tx.send(wakes).unwrap();
                    }

                    let start = Instant::now();
                    barrier.wait();
                    runtime.block_on(async {
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                    elapsed += start.elapsed();
                }

                elapsed
            });

            drop(command_txs);
            for producer in producer_threads {
                producer.join().unwrap();
            }
        });
    }

    group.finish();
}

/// Measures whether a remotely woken task returns to the LLC containing its
/// working set. Every task repeatedly traverses a randomized pointer cycle,
/// making each load depend on the preceding load and defeating sequential
/// prefetching. The configured working set is larger than typical private
/// caches but small enough for multiple tasks to remain resident in an LLC.
#[cfg(all(tokio_unstable, target_os = "linux"))]
fn cache_affine_wake(c: &mut Criterion) {
    let (workers, topology) = benchmark_topology();
    let partitions = topology.partition_count();
    let cache_bytes = std::env::var("TOKIO_LLC_BENCH_CACHE_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|bytes| *bytes >= 2 * std::mem::size_of::<usize>())
        .unwrap_or(CACHE_BYTES_PER_TASK);
    let tasks_per_partition = std::env::var("TOKIO_LLC_BENCH_CACHE_TASKS_PER_LLC")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|tasks| *tasks > 0)
        .unwrap_or(CACHE_TASKS_PER_PARTITION);
    let tasks = partitions * tasks_per_partition;
    let bytes_touched = tasks * cache_bytes * CACHE_ROUNDS;
    let worker_cpus = benchmark_worker_cpus(workers, partitions)
        .expect("the cache benchmark requires Linux sysfs topology");
    let mut group = c.benchmark_group(format!(
        "llc_aware/cache_affine_wake/{workers}_workers/{cache_bytes}_bytes/{tasks_per_partition}_tasks_per_llc"
    ));
    group.throughput(Throughput::Bytes(bytes_touched as u64));

    for mode in [Mode::Disabled, Mode::Enabled] {
        let runtime = pinned_runtime(mode, workers, topology.clone(), worker_cpus.clone());
        group.bench_with_input(BenchmarkId::from_parameter(mode.name()), &mode, |b, _| {
            b.iter_custom(|iterations| {
                let mut elapsed = Duration::ZERO;

                for iteration in 0..iterations {
                    let (ready_tx, ready_rx) = mpsc::channel::<Waker>();
                    let mut handles = Vec::with_capacity(tasks);

                    for task in 0..tasks {
                        let ready_tx = ready_tx.clone();
                        let partition = task % partitions;
                        let seed = iteration.wrapping_mul(tasks as u64).wrapping_add(task as u64);
                        handles.push(
                            tokio::task::Builder::new()
                                .llc_partition(partition)
                                .spawn_on(
                                    async move {
                                        // Allocation and first touch happen on the target worker
                                        // before timing begins.
                                        let links = pointer_cycle(cache_bytes, seed);
                                        let mut cursor = chase_pointers(&links, task % links.len());

                                        for _ in 0..CACHE_ROUNDS {
                                            wait_for_external_wake(&ready_tx).await;
                                            cursor = chase_pointers(&links, cursor);
                                        }

                                        (links, black_box(cursor))
                                    },
                                    runtime.handle(),
                                )
                                .unwrap(),
                        );
                    }
                    drop(ready_tx);

                    // Receiving one waker per task proves that every task has
                    // initialized and returned Pending before timing begins.
                    let mut wakers = receive_wakers(&ready_rx, tasks);
                    let start = Instant::now();
                    for round in 0..CACHE_ROUNDS {
                        for waker in wakers.drain(..) {
                            waker.wake();
                        }
                        if round + 1 < CACHE_ROUNDS {
                            wakers = receive_wakers(&ready_rx, tasks);
                        }
                    }
                    let completed = runtime.block_on(async {
                        let mut completed = Vec::with_capacity(tasks);
                        for handle in handles {
                            completed.push(handle.await.unwrap());
                        }
                        completed
                    });
                    elapsed += start.elapsed();
                    // Keep destruction of the working sets outside the timed
                    // region; allocator behavior is not under test.
                    drop(black_box(completed));
                }

                elapsed
            });
        });
    }

    group.finish();
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
async fn wait_for_external_wake(ready: &mpsc::Sender<Waker>) {
    let mut registered = false;
    poll_fn(|cx| {
        if registered {
            Poll::Ready(())
        } else {
            registered = true;
            ready.send(cx.waker().clone()).unwrap();
            Poll::Pending
        }
    })
    .await
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn receive_wakers(ready: &mpsc::Receiver<Waker>, tasks: usize) -> Vec<Waker> {
    (0..tasks).map(|_| ready.recv().unwrap()).collect()
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn pointer_cycle(bytes: usize, seed: u64) -> Vec<usize> {
    let words = (bytes / std::mem::size_of::<usize>()).max(2);
    let mut order = (0..words).collect::<Vec<_>>();
    let mut random = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);

    for index in (1..words).rev() {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        order.swap(index, random as usize % (index + 1));
    }

    let mut links = vec![0; words];
    for pair in order.windows(2) {
        links[pair[0]] = pair[1];
    }
    links[*order.last().unwrap()] = order[0];
    links
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn chase_pointers(links: &[usize], mut cursor: usize) -> usize {
    for _ in 0..links.len() {
        cursor = links[cursor];
    }
    black_box(cursor)
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn benchmark_topology() -> (usize, LlcAwareConfig) {
    let topology = LlcAwareConfig::from_linux_topology()
        .expect("the LLC benchmark requires Linux sysfs topology");
    let cpus_by_llc = benchmark_cpus_by_llc()
        .expect("the LLC benchmark requires Linux CPU affinity information");
    assert_eq!(
        cpus_by_llc.len(),
        topology.partition_count(),
        "the allowed CPU set does not cover every discovered LLC",
    );
    let default_workers = cpus_by_llc.values().map(Vec::len).sum();
    let workers = std::env::var("TOKIO_LLC_BENCH_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_workers);
    assert!(
        workers >= topology.partition_count(),
        "TOKIO_LLC_BENCH_WORKERS={workers} is smaller than the {} discovered LLCs",
        topology.partition_count(),
    );
    (workers, topology)
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn benchmark_tasks() -> usize {
    let allowed_cpus = benchmark_cpus_by_llc()
        .expect("the LLC benchmark requires Linux CPU affinity information")
        .values()
        .map(Vec::len)
        .sum::<usize>();
    std::env::var("TOKIO_LLC_BENCH_TASKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|tasks| *tasks > 0)
        .unwrap_or_else(|| (allowed_cpus * DEFAULT_TASKS_PER_CPU).max(DEFAULT_TASKS))
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn benchmark_task_work() -> usize {
    std::env::var("TOKIO_LLC_BENCH_TASK_WORK")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn run_task_work(iterations: usize, seed: usize) {
    let mut value = black_box(seed ^ 0x9e37_79b9);
    for _ in 0..iterations {
        value ^= value.wrapping_shl(13);
        value ^= value.wrapping_shr(7);
        value ^= value.wrapping_shl(17);
    }
    black_box(value);
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn benchmark_producers(partitions: usize) -> usize {
    let cpus_by_llc = benchmark_cpus_by_llc()
        .expect("the LLC benchmark requires Linux CPU affinity information");
    assert_eq!(
        cpus_by_llc.len(),
        partitions,
        "the allowed CPU set does not cover every discovered LLC",
    );
    let producers_per_partition = cpus_by_llc
        .values()
        .map(Vec::len)
        .min()
        .unwrap()
        .min(DEFAULT_PRODUCERS_PER_PARTITION);
    let default_producers = partitions * producers_per_partition;
    let producers = std::env::var("TOKIO_LLC_BENCH_PRODUCERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|producers| *producers > 0)
        .unwrap_or(default_producers);
    assert_eq!(
        producers % partitions,
        0,
        "TOKIO_LLC_BENCH_PRODUCERS={producers} is not a multiple of {partitions} LLCs",
    );
    assert!(
        producers / partitions <= cpus_by_llc.values().map(Vec::len).min().unwrap(),
        "TOKIO_LLC_BENCH_PRODUCERS={producers} exceeds the balanced allowed CPU capacity",
    );
    producers
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn producer_task_range(
    tasks: usize,
    producers: usize,
    producer: usize,
) -> std::ops::Range<usize> {
    let tasks_per_producer = tasks / producers;
    let remainder = tasks % producers;
    let start = producer * tasks_per_producer + producer.min(remainder);
    let len = tasks_per_producer + usize::from(producer < remainder);
    start..start + len
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn pinned_runtime(
    mode: Mode,
    workers: usize,
    topology: LlcAwareConfig,
    worker_cpus: Arc<[usize]>,
) -> Runtime {
    let next_worker = Arc::new(AtomicUsize::new(0));
    let mut builder = Builder::new_multi_thread();
    builder.worker_threads(workers).enable_all();
    builder.on_thread_start(move || {
        let worker = next_worker.fetch_add(1, Relaxed);
        if let Some(&cpu) = worker_cpus.get(worker) {
            pin_current_thread(cpu);
        }
    });
    match mode {
        Mode::Disabled => {
            builder.disable_llc_aware();
        }
        Mode::Enabled => {
            builder.llc_aware(topology);
        }
    }
    builder.build().unwrap()
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn benchmark_worker_cpus(workers: usize, partitions: usize) -> io::Result<Arc<[usize]>> {
    let cpus_by_llc = benchmark_cpus_by_llc()?;
    if cpus_by_llc.len() != partitions {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "found {} LLC CPU groups for {partitions} partitions",
                cpus_by_llc.len()
            ),
        ));
    }

    // Select CPUs round-robin by LLC so the first `partitions` workers place
    // exactly one worker in every LLC. Additional workers remain balanced.
    let mut selected = Vec::with_capacity(workers);
    let mut offset = 0;
    while selected.len() < workers {
        let previous_len = selected.len();
        for cpus in cpus_by_llc.values() {
            if let Some(&cpu) = cpus.get(offset) {
                selected.push(cpu);
                if selected.len() == workers {
                    break;
                }
            }
        }
        if selected.len() == previous_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{workers} threads exceed the allowed CPU count"),
            ));
        }
        offset += 1;
    }
    Ok(selected.into())
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn benchmark_cpus_by_llc() -> io::Result<BTreeMap<String, Vec<usize>>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let allowed = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "allowed CPU list is missing"))?;
    let mut cpus_by_llc = BTreeMap::<String, Vec<usize>>::new();
    for cpu in parse_cpu_list(allowed.trim())? {
        cpus_by_llc.entry(llc_key(cpu)?).or_default().push(cpu);
    }
    Ok(cpus_by_llc)
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn pin_current_thread(cpu: usize) {
    assert!(
        cpu < libc::CPU_SETSIZE as usize,
        "CPU {cpu} exceeds cpu_set_t capacity"
    );
    // SAFETY: `cpu_set_t` is initialized before use, `CPU_SET` receives a CPU
    // found in this process's allowed CPU set, and pthread_self is valid for
    // the duration of pthread_setaffinity_np.
    let result = unsafe {
        let mut set = std::mem::zeroed::<libc::cpu_set_t>();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        libc::pthread_setaffinity_np(
            libc::pthread_self(),
            std::mem::size_of::<libc::cpu_set_t>(),
            &set,
        )
    };
    assert_eq!(result, 0, "failed to pin runtime worker to CPU {cpu}");
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn llc_key(cpu: usize) -> io::Result<String> {
    let cache_dir = format!("/sys/devices/system/cpu/cpu{cpu}/cache");
    let mut best = None::<(u32, String)>;

    for entry in std::fs::read_dir(cache_dir)? {
        let path = entry?.path();
        let Ok(level) = read_trimmed(path.join("level")).and_then(|level| {
            level
                .parse::<u32>()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        }) else {
            continue;
        };
        let Ok(kind) = read_trimmed(path.join("type")) else {
            continue;
        };
        if kind != "Unified" && kind != "Data" {
            continue;
        }

        let Ok(identity) =
            read_trimmed(path.join("id")).or_else(|_| read_trimmed(path.join("shared_cpu_list")))
        else {
            continue;
        };
        if best
            .as_ref()
            .map_or(true, |(best_level, _)| level > *best_level)
        {
            best = Some((level, format!("{level}:{identity}")));
        }
    }

    if let Some((_, key)) = best {
        return Ok(key);
    }

    read_trimmed(format!(
        "/sys/devices/system/cpu/cpu{cpu}/topology/llc_id"
    ))
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn read_trimmed(path: impl AsRef<std::path::Path>) -> io::Result<String> {
    Ok(std::fs::read_to_string(path)?.trim().to_owned())
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn parse_cpu_list(list: &str) -> io::Result<Vec<usize>> {
    let mut cpus = Vec::new();
    for range in list.split(',') {
        let (start, end) = match range.split_once('-') {
            Some((start, end)) => (parse_cpu(start)?, parse_cpu(end)?),
            None => {
                let cpu = parse_cpu(range)?;
                (cpu, cpu)
            }
        };
        if start > end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid CPU range",
            ));
        }
        cpus.extend(start..=end);
    }
    Ok(cpus)
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
fn parse_cpu(value: &str) -> io::Result<usize> {
    value
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(all(tokio_unstable, target_os = "linux"))]
criterion_group!(
    llc_aware_benches,
    local_spawn,
    remote_spawn,
    hinted_spawn,
    same_llc_wake,
    cross_llc_wake,
    work_stealing,
    cache_affine_wake
);
#[cfg(all(tokio_unstable, target_os = "linux"))]
criterion_main!(llc_aware_benches);

#[cfg(not(all(tokio_unstable, target_os = "linux")))]
fn main() {
    eprintln!("rt_llc_aware requires Linux and RUSTFLAGS=\"--cfg tokio_unstable\"");
}
