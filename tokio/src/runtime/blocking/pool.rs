//! Thread pool for blocking operations

use crate::loom::sync::{Arc, Condvar, Mutex};
use crate::loom::thread;
use crate::runtime::blocking::schedule::BlockingSchedule;
use crate::runtime::blocking::sharded::ShardedImpl;
use crate::runtime::blocking::{shutdown, BlockingTask};
use crate::runtime::builder::ThreadNameFn;
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::{AutoBox, Builder, Callback, Handle};
use crate::util::metric_atomics::MetricAtomicUsize;
#[cfg(all(tokio_unstable, target_has_atomic = "64"))]
use crate::util::metric_atomics::MetricAtomicU64;
use crate::util::trace::{blocking_task, SpawnMeta};

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io;
use std::sync::atomic::Ordering;
use std::time::Duration;
#[cfg(all(tokio_unstable, target_has_atomic = "64"))]
use std::time::Instant;

pub(crate) struct BlockingPool {
    spawner: Spawner,
    shutdown_rx: shutdown::Receiver,
}

#[derive(Clone)]
pub(crate) struct Spawner {
    inner: Arc<Inner>,
}

#[derive(Default)]
pub(crate) struct SpawnerMetrics {
    num_threads: MetricAtomicUsize,
    num_idle_threads: MetricAtomicUsize,
    queue_depth: MetricAtomicUsize,
    #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
    track_queue_latency: bool,
    #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
    queue_latency_ns: MetricAtomicU64,
    #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
    queue_latency_observations: MetricAtomicU64,
}

impl SpawnerMetrics {
    pub(super) fn num_threads(&self) -> usize {
        self.num_threads.load(Ordering::Relaxed)
    }

    pub(super) fn num_idle_threads(&self) -> usize {
        self.num_idle_threads.load(Ordering::Relaxed)
    }

    cfg_unstable_metrics! {
        fn queue_depth(&self) -> usize {
            self.queue_depth.load(Ordering::Relaxed)
        }
    }

    fn inc_num_threads(&self) {
        self.num_threads.increment();
    }

    pub(super) fn dec_num_threads(&self) {
        self.num_threads.decrement();
    }

    pub(super) fn inc_num_idle_threads(&self) {
        self.num_idle_threads.increment();
    }

    pub(super) fn dec_num_idle_threads(&self) -> usize {
        self.num_idle_threads.decrement()
    }

    pub(super) fn inc_queue_depth(&self) {
        self.queue_depth.increment();
    }

    pub(super) fn dec_queue_depth(&self) {
        self.queue_depth.decrement();
    }
}

struct Inner {
    /// Queue + notification implementation.
    inner_impl: InnerImpl,

    /// Spawned threads use this name.
    thread_name: ThreadNameFn,

    /// Spawned thread stack size.
    stack_size: Option<usize>,

    /// Call after a thread starts.
    after_start: Option<Callback>,

    /// Call before a thread stops.
    before_stop: Option<Callback>,

    // Maximum number of threads.
    thread_cap: usize,

    // Number of runtime scheduler workers counted in `num_threads`.
    scheduler_threads: usize,

    // Customizable wait timeout.
    keep_alive: Duration,

    // Metrics about the pool.
    metrics: SpawnerMetrics,
}

/// Per-variant queue + notification + lock topology.
enum InnerImpl {
    Locked(LockedImpl),
    Sharded(ShardedImpl),
}

/// Single-mutex + condvar implementation.
struct LockedImpl {
    mutex: Mutex<LockedInner>,
    condvar: Condvar,
}

struct LockedInner {
    queue: VecDeque<Task>,
    num_notify: u32,
    /// Thread-management state. Split apart so that it's re-usable in a
    /// sharded queue implementation.
    thread_mgmt_state: ThreadManagementState,
}

/// State handed back from `InnerImpl::begin_shutdown` to the caller so it
/// can join the worker threads after the wait completes: an optional
/// previously-timed-out worker, plus the map of currently-running workers.
pub(super) type ShutdownHandles = (
    Option<thread::JoinHandle<()>>,
    HashMap<usize, thread::JoinHandle<()>>,
);

/// Thread-management state used by every `InnerImpl` variant.
pub(super) struct ThreadManagementState {
    pub(super) shutdown: bool,
    pub(super) shutdown_tx: Option<shutdown::Sender>,
    /// Prior to shutdown, we clean up `JoinHandles` by having each timed-out
    /// thread join on the previous timed-out thread. This is not strictly
    /// necessary but helps avoid Valgrind false positives, see
    /// <https://github.com/tokio-rs/tokio/commit/646fbae76535e397ef79dbcaacb945d4c829f666>
    /// for more information.
    pub(super) last_exiting_thread: Option<thread::JoinHandle<()>>,
    /// This holds the `JoinHandles` for all running threads; on shutdown, the thread
    /// calling shutdown handles joining on these.
    pub(super) worker_threads: HashMap<usize, thread::JoinHandle<()>>,
    /// This is a counter used to iterate `worker_threads` in a consistent order (for loom's
    /// benefit).
    pub(super) worker_thread_index: usize,
}

impl ThreadManagementState {
    /// Flag the pool as shutting down and hand back the worker `JoinHandle`s
    /// to join. Returns `None` if shutdown has already begun. The caller is
    /// responsible for waking all waiting workers.
    pub(super) fn begin_shutdown(&mut self) -> Option<ShutdownHandles> {
        if self.shutdown {
            return None;
        }
        self.shutdown = true;
        self.shutdown_tx = None;

        let last_exited_thread = std::mem::take(&mut self.last_exiting_thread);
        let workers = std::mem::take(&mut self.worker_threads);
        Some((last_exited_thread, workers))
    }

    /// Bookkeeping for a worker exiting on its keep-alive timeout: leaves
    /// its own handle for the next timed-out worker (or shutdown) to join,
    /// and returns the previous timed-out thread's handle, which the caller
    /// must join after releasing the lock.
    pub(super) fn worker_timed_out(
        &mut self,
        worker_thread_id: usize,
    ) -> Option<thread::JoinHandle<()>> {
        let my_handle = self.worker_threads.remove(&worker_thread_id);
        std::mem::replace(&mut self.last_exiting_thread, my_handle)
    }
}

pub(crate) struct Task {
    task: task::UnownedTask<BlockingSchedule>,
    mandatory: Mandatory,
    #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
    submitted_at: Option<Instant>,
}

#[derive(PartialEq, Eq)]
pub(crate) enum Mandatory {
    #[cfg_attr(any(not(feature = "fs"), target_os = "emscripten"), allow(dead_code))]
    Mandatory,
    NonMandatory,
}

pub(crate) enum SpawnError {
    /// Pool is shutting down and the task was not scheduled
    ShuttingDown,
    /// There are no worker threads available to take the task
    /// and the OS failed to spawn a new one
    NoThreads(io::Error),
}

impl From<SpawnError> for io::Error {
    fn from(e: SpawnError) -> Self {
        match e {
            SpawnError::ShuttingDown => io::Error::other("blocking pool shutting down"),
            SpawnError::NoThreads(e) => e,
        }
    }
}

#[cfg(all(tokio_unstable, target_has_atomic = "64"))]
#[inline]
fn now() -> Option<Instant> {
    if cfg!(all(
        target_arch = "wasm32",
        target_os = "unknown",
        target_vendor = "unknown"
    )) {
        None
    } else {
        Some(Instant::now())
    }
}

#[cfg(all(tokio_unstable, target_has_atomic = "64"))]
#[inline]
fn duration_as_u64(dur: Duration) -> u64 {
    u64::try_from(dur.as_nanos()).unwrap_or(u64::MAX)
}

impl Task {
    pub(crate) fn new(task: task::UnownedTask<BlockingSchedule>, mandatory: Mandatory) -> Task {
        Task {
            task,
            mandatory,
            #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
            submitted_at: None,
        }
    }

    pub(super) fn shutdown(self) {
        self.task.shutdown();
    }

    #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
    fn record_submission(&mut self, metrics: &SpawnerMetrics) {
        if metrics.track_queue_latency {
            self.submitted_at = now();
        }
    }

    pub(super) fn run(self, _metrics: &SpawnerMetrics) {
        #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
        if let Some(submitted_at) = self.submitted_at {
            // These counters do not synchronize task data. They wrap on overflow,
            // and separate reads do not form an atomic snapshot.
            _metrics.queue_latency_ns.add(duration_as_u64(submitted_at.elapsed()), Ordering::Relaxed);
            _metrics.queue_latency_observations.add(1, Ordering::Relaxed);
        }
        self.task.run();
    }

    pub(super) fn shutdown_or_run_if_mandatory(self, metrics: &SpawnerMetrics) {
        match self.mandatory {
            Mandatory::NonMandatory => self.task.shutdown(),
            Mandatory::Mandatory => self.run(metrics),
        }
    }
}

#[cfg(all(test, not(loom), tokio_unstable, target_has_atomic = "64"))]
mod latency_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;

    fn exercise(sharded: bool, enabled: bool, multi_thread: bool) {
        #[cfg(feature = "rt-multi-thread")]
        let mut builder = if multi_thread {
            Builder::new_multi_thread()
        } else {
            Builder::new_current_thread()
        };
        #[cfg(not(feature = "rt-multi-thread"))]
        let mut builder = {
            assert!(!multi_thread);
            Builder::new_current_thread()
        };
        builder.sharded_blocking_queue = sharded;
        builder.track_blocking_queue_latency = enabled;
        builder.max_blocking_threads(1);
        let rt = builder.build().unwrap();
        let spawner = rt.handle().inner.blocking_spawner().clone();
        let metrics = &spawner.inner.metrics;

        let first = rt.spawn_blocking(|| {});
        rt.block_on(first).unwrap();
        assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), u64::from(enabled));

        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocker = rt.spawn_blocking(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        started_rx.recv().unwrap();

        let cancelled = rt.spawn_blocking(|| panic!("cancelled task ran"));
        cancelled.abort();
        let mut submitters = Vec::new();
        for _ in 0..4 {
            let handle = rt.handle().clone();
            submitters.push(std::thread::spawn(move || handle.spawn_blocking(|| {})));
        }
        let tasks: Vec<_> = submitters.into_iter().map(|t| t.join().unwrap()).collect();
        release_tx.send(()).unwrap();
        rt.block_on(async {
            blocker.await.unwrap();
            assert!(cancelled.await.unwrap_err().is_cancelled());
            for task in tasks {
                task.await.unwrap();
            }
            let error = crate::task::spawn_blocking(|| panic!("blocking task panic"))
                .await
                .unwrap_err();
            assert!(error.is_panic());
        });
        assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), if enabled { 8 } else { 0 });
        if enabled {
            assert!(metrics.queue_latency_ns.load(Ordering::Relaxed) > 0);
        } else {
            assert_eq!(metrics.queue_latency_ns.load(Ordering::Relaxed), 0);
        }
    }

    #[test]
    fn normal_dequeues_are_recorded_once() {
        for sharded in [false, true] {
            for enabled in [false, true] {
                exercise(sharded, enabled, false);
                #[cfg(feature = "rt-multi-thread")]
                exercise(sharded, enabled, true);
            }
        }
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test]
    fn scheduler_workers_are_not_observed() {
        for sharded in [false, true] {
            let mut builder = Builder::new_multi_thread();
            builder.worker_threads(1);
            builder.sharded_blocking_queue = sharded;
            builder.track_blocking_queue_latency = true;
            let rt = builder.build().unwrap();
            let spawner = rt.handle().inner.blocking_spawner().clone();
            let metrics = &spawner.inner.metrics;

            rt.block_on(async {
                crate::spawn(async {
                    crate::task::block_in_place(|| {
                        let (tx, rx) = mpsc::channel();
                        crate::spawn(async move { tx.send(()).unwrap() });
                        rx.recv().unwrap();
                    });
                }).await.unwrap();
                assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), 0);
                crate::task::spawn_blocking(|| {}).await.unwrap();
            });
            assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), 1);
        }
    }

    #[cfg(all(feature = "rt-multi-thread", target_os = "linux", not(miri)))]
    #[test]
    fn scheduler_start_failure_returns_error_for_each_queue() {
        for sharded in [false, true] {
            let tracker = Arc::new(());
            let mut builder = Builder::new_multi_thread();
            builder.sharded_blocking_queue = sharded;
            builder.worker_threads(1).thread_stack_size(usize::MAX).on_thread_start({
                let tracker = tracker.clone();
                move || { let _ = &tracker; }
            });
            assert!(builder.build().is_err());
            drop(builder);
            assert_eq!(Arc::strong_count(&tracker), 1);
        }
    }

    #[test]
    fn queue_latency_accumulates_delay() {
        for sharded in [false, true] {
            let mut builder = Builder::new_current_thread();
            builder.sharded_blocking_queue = sharded;
            builder.track_blocking_queue_latency = true;
            builder.max_blocking_threads(1);
            let rt = builder.build().unwrap();
            let spawner = rt.handle().inner.blocking_spawner().clone();
            let metrics = &spawner.inner.metrics;

            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let blocker = rt.spawn_blocking(move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
            started_rx.recv().unwrap();

            let delayed_task = rt.spawn_blocking(|| {});
            // Hold the worker for at least 5ms while delayed_task is queued
            std::thread::sleep(Duration::from_millis(5));
            release_tx.send(()).unwrap();

            rt.block_on(async {
                blocker.await.unwrap();
                delayed_task.await.unwrap();
            });

            assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), 2);
            assert!(
                metrics.queue_latency_ns.load(Ordering::Relaxed) >= 5_000_000,
                "expected at least 5ms, got {}ns",
                metrics.queue_latency_ns.load(Ordering::Relaxed)
            );
        }
    }

    #[test]
    fn shutdown_records_only_tasks_that_run() {
        for sharded in [false, true] {
            let mut builder = Builder::new_current_thread();
            builder.sharded_blocking_queue = sharded;
            builder.track_blocking_queue_latency = true;
            builder.max_blocking_threads(1);
            let rt = builder.build().unwrap();
            let spawner = rt.handle().inner.blocking_spawner().clone();
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let blocker = rt.spawn_blocking(move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
            started_rx.recv().unwrap();
            let tasks: Vec<_> = (0..4).map(|_| rt.spawn_blocking(|| ())).collect();
            rt.shutdown_background();
            release_tx.send(()).unwrap();

            let waiter = Builder::new_current_thread().build().unwrap();
            let completed = waiter.block_on(async {
                blocker.await.unwrap();
                let mut completed = 0;
                for task in tasks {
                    match task.await {
                        Ok(()) => completed += 1,
                        Err(error) => assert!(error.is_cancelled()),
                    }
                }
                completed
            });
            assert_eq!(spawner.inner.metrics.queue_latency_observations.load(Ordering::Relaxed), 1 + completed);
        }
    }

    #[test]
    fn mandatory_tasks_run_and_are_observed_on_shutdown() {
        let metrics = SpawnerMetrics {
            track_queue_latency: true,
            ..SpawnerMetrics::default()
        };
        let rt = Builder::new_current_thread().build().unwrap();
        let id = task::Id::next();
        let ran = Arc::new(AtomicBool::new(false));
        let ran_clone = ran.clone();
        let f: Box<dyn FnOnce() + Send> = Box::new(move || {
            ran_clone.store(true, Ordering::SeqCst);
        });
        let fut = blocking_task::<Box<dyn FnOnce() + Send>, _>(
            BlockingTask::new(f),
            SpawnMeta::new_unnamed(0),
            id.as_u64(),
        );
        let (unowned, handle) = task::unowned(
            fut,
            BlockingSchedule::new(rt.handle()),
            id,
            task::SpawnLocation::capture(),
        );
        let mut task = Task::new(unowned, Mandatory::Mandatory);
        task.record_submission(&metrics);
        assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), 0);

        // Mandatory tasks must run and be observed even during shutdown
        task.shutdown_or_run_if_mandatory(&metrics);

        assert!(ran.load(Ordering::SeqCst));
        assert_eq!(metrics.queue_latency_observations.load(Ordering::Relaxed), 1);

        rt.block_on(async {
            handle.await.unwrap();
        });
    }
}

const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Runs the provided function on an executor dedicated to blocking operations.
/// Tasks will be scheduled as non-mandatory, meaning they may not get executed
/// in case of runtime shutdown.
#[track_caller]
#[cfg_attr(target_os = "wasi", allow(dead_code))]
pub(crate) fn spawn_blocking<F, R>(func: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let rt = Handle::current();
    rt.spawn_blocking(func)
}

cfg_io_blocking! {
    #[cfg_attr(any(
        all(loom, not(test)), // the function is covered by loom tests
        test,
        target_os = "emscripten", // fs and io-std use the inline shim
    ), allow(dead_code))]
    /// Runs the provided function on an executor dedicated to blocking
    /// operations. Tasks will be scheduled as mandatory, meaning they are
    /// guaranteed to run unless a shutdown is already taking place. In case a
    /// shutdown is already taking place, `None` will be returned.
    pub(crate) fn spawn_mandatory_blocking<F, R>(func: F) -> Option<JoinHandle<R>>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let rt = Handle::current();
        rt.inner.blocking_spawner().spawn_mandatory_blocking(&rt, func)
    }
}

// ===== impl BlockingPool =====

impl BlockingPool {
    pub(crate) fn new(
        builder: &Builder,
        thread_cap: usize,
        scheduler_threads: usize,
    ) -> BlockingPool {
        let (shutdown_tx, shutdown_rx) = shutdown::channel();
        let keep_alive = builder.keep_alive.unwrap_or(KEEP_ALIVE);

        let thread_mgmt_state = ThreadManagementState {
            shutdown: false,
            shutdown_tx: Some(shutdown_tx),
            last_exiting_thread: None,
            worker_threads: HashMap::new(),
            worker_thread_index: 0,
        };

        let inner_impl = if builder.sharded_blocking_queue {
            InnerImpl::Sharded(ShardedImpl::new(thread_mgmt_state))
        } else {
            InnerImpl::Locked(LockedImpl::new(thread_mgmt_state))
        };

        BlockingPool {
            spawner: Spawner {
                inner: Arc::new(Inner {
                    inner_impl,
                    thread_name: builder.thread_name.clone(),
                    stack_size: builder.thread_stack_size,
                    after_start: builder.after_start.clone(),
                    before_stop: builder.before_stop.clone(),
                    thread_cap,
                    scheduler_threads,
                    keep_alive,
                    metrics: SpawnerMetrics {
                        #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
                        track_queue_latency: builder.track_blocking_queue_latency,
                        ..SpawnerMetrics::default()
                    },
                }),
            },
            shutdown_rx,
        }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    pub(crate) fn shutdown(&mut self, timeout: Option<Duration>) {
        // The function can be called multiple times. First, by explicitly
        // calling `shutdown` then by the drop handler calling `shutdown`. This
        // prevents shutting down twice.
        let inner = &self.spawner.inner;
        let (last_exited_thread, workers) = match inner.inner_impl.begin_shutdown(&inner.metrics) {
            Some(x) => x,
            None => return,
        };

        if self.shutdown_rx.wait(timeout) {
            let _ = last_exited_thread.map(thread::JoinHandle::join);

            // Loom requires that execution be deterministic, so sort by thread ID before joining.
            // (HashMaps use a randomly-seeded hash function, so the order is nondeterministic)
            #[cfg(loom)]
            let workers: Vec<(usize, thread::JoinHandle<()>)> = {
                let mut workers: Vec<_> = workers.into_iter().collect();
                workers.sort_by_key(|(id, _)| *id);
                workers
            };

            for (_id, handle) in workers {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for BlockingPool {
    fn drop(&mut self) {
        self.shutdown(None);
    }
}

impl fmt::Debug for BlockingPool {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BlockingPool").finish()
    }
}

// ===== impl Spawner =====

impl Spawner {
    #[track_caller]
    pub(crate) fn spawn_blocking<F, R>(&self, rt: &Handle, func: F, is_scheduler_worker: bool) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let fn_size = std::mem::size_of::<F>();
        let (join_handle, spawn_result) = if AutoBox::<F>::SHOULD_BOX {
            self.spawn_blocking_inner(
                Box::new(func),
                Mandatory::NonMandatory,
                SpawnMeta::new_unnamed(fn_size),
                rt,
                is_scheduler_worker,
            )
        } else {
            self.spawn_blocking_inner(
                func,
                Mandatory::NonMandatory,
                SpawnMeta::new_unnamed(fn_size),
                rt,
                is_scheduler_worker,
            )
        };

        match spawn_result {
            Ok(()) => join_handle,
            // Compat: do not panic here, return the join_handle even though it will never resolve
            Err(SpawnError::ShuttingDown) => join_handle,
            Err(SpawnError::NoThreads(e)) => {
                panic!("OS can't spawn worker thread: {e}")
            }
        }
    }

    cfg_io_blocking! {
        #[track_caller]
        #[cfg_attr(any(
            all(loom, not(test)), // the function is covered by loom tests
            test,
            target_os = "emscripten", // fs and io-std use the inline shim
        ), allow(dead_code))]
        pub(crate) fn spawn_mandatory_blocking<F, R>(&self, rt: &Handle, func: F) -> Option<JoinHandle<R>>
        where
            F: FnOnce() -> R + Send + 'static,
            R: Send + 'static,
        {
            let fn_size = std::mem::size_of::<F>();
            let (join_handle, spawn_result) = if AutoBox::<F>::SHOULD_BOX {
                self.spawn_blocking_inner(
                    Box::new(func),
                    Mandatory::Mandatory,
                    SpawnMeta::new_unnamed(fn_size),
                    rt,
                    false,
                )
            } else {
                self.spawn_blocking_inner(
                    func,
                    Mandatory::Mandatory,
                    SpawnMeta::new_unnamed(fn_size),
                    rt,
                    false,
                )
            };

            if spawn_result.is_ok() {
                Some(join_handle)
            } else {
                None
            }
        }
    }

    #[track_caller]
    pub(crate) fn spawn_blocking_inner<F, R>(
        &self,
        func: F,
        is_mandatory: Mandatory,
        spawn_meta: SpawnMeta<'_>,
        rt: &Handle,
        is_scheduler_worker: bool,
    ) -> (JoinHandle<R>, Result<(), SpawnError>)
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let id = task::Id::next();
        let fut =
            blocking_task::<F, BlockingTask<F>>(BlockingTask::new(func), spawn_meta, id.as_u64());

        let (task, handle) = task::unowned(
            fut,
            BlockingSchedule::new(rt),
            id,
            task::SpawnLocation::capture(),
        );

        let task = Task::new(task, is_mandatory);
        #[cfg(all(tokio_unstable, target_has_atomic = "64"))]
        let task = {
            let mut task = task;
            if !is_scheduler_worker {
                task.record_submission(&self.inner.metrics);
            }
            task
        };
        #[cfg(not(all(tokio_unstable, target_has_atomic = "64")))]
        let _ = is_scheduler_worker;
        let spawned = self.spawn_task(task, rt);
        (handle, spawned)
    }

    pub(crate) fn num_blocking_threads(&self) -> usize {
        self.inner
            .metrics
            .num_threads()
            .saturating_sub(self.inner.scheduler_threads)
    }

    #[cfg(all(test, not(loom), not(target_family = "wasm")))]
    pub(super) fn sharded_queue(&self) -> (&ShardedImpl, &SpawnerMetrics) {
        match &self.inner.inner_impl {
            InnerImpl::Sharded(queue) => (queue, &self.inner.metrics),
            InnerImpl::Locked(_) => panic!("expected a sharded blocking queue"),
        }
    }

    fn spawn_task(&self, task: Task, rt: &Handle) -> Result<(), SpawnError> {
        // The `on_no_idle` closure runs under the same lock as the queue
        // push, exactly like the pre-refactor code that called
        // `self.spawn_thread` directly while holding `Mutex<ThreadManagementState>`.
        self.inner
            .inner_impl
            .spawn_task(task, &self.inner.metrics, |thread_mgmt_state| {
                // No threads are able to process the task.

                if self.inner.metrics.num_threads() == self.inner.thread_cap {
                    // At max number of threads
                    return Ok(());
                }

                assert!(thread_mgmt_state.shutdown_tx.is_some());
                let shutdown_tx = thread_mgmt_state.shutdown_tx.clone();

                if let Some(shutdown_tx) = shutdown_tx {
                    let id = thread_mgmt_state.worker_thread_index;

                    match self.spawn_thread(shutdown_tx, rt, id) {
                        Ok(handle) => {
                            self.inner.metrics.inc_num_threads();
                            thread_mgmt_state.worker_thread_index += 1;
                            thread_mgmt_state.worker_threads.insert(id, handle);
                        }
                        Err(ref e)
                            if is_temporary_os_thread_error(e)
                                && self.num_blocking_threads() > 0 =>
                        {
                            // OS temporarily failed to spawn a new thread.
                            // The task will be picked up eventually by a currently
                            // busy thread.
                        }
                        Err(e) => {
                            // The OS refused to spawn the thread and there is no thread
                            // to pick up the task that has just been pushed to the queue.
                            return Err(SpawnError::NoThreads(e));
                        }
                    }
                }

                Ok(())
            })
    }

    fn spawn_thread(
        &self,
        shutdown_tx: shutdown::Sender,
        rt: &Handle,
        id: usize,
    ) -> io::Result<thread::JoinHandle<()>> {
        let mut builder = thread::Builder::new().name((self.inner.thread_name)());

        if let Some(stack_size) = self.inner.stack_size {
            builder = builder.stack_size(stack_size);
        }

        let rt = rt.clone();

        builder.spawn(move || {
            // Only the reference should be moved into the closure
            let _enter = rt.enter();
            rt.inner.blocking_spawner().inner.run(id);
            drop(shutdown_tx);
        })
    }
}

cfg_unstable_metrics! {
    impl Spawner {
        pub(crate) fn num_idle_threads(&self) -> usize {
            self.inner.metrics.num_idle_threads()
        }

        pub(crate) fn queue_depth(&self) -> usize {
            self.inner.metrics.queue_depth()
        }

    }
}

// Each method on `InnerImpl` dispatches to the matching method on the
// concrete variant. The variant methods own the entire critical section
// for their operation, which makes it self-evident that the `Locked`
// variant's behavior is identical to the pre-refactor code and gives a
// future `Concurrent` variant a symmetric slot to fill.
impl InnerImpl {
    fn spawn_task<F>(
        &self,
        task: Task,
        metrics: &SpawnerMetrics,
        on_no_idle: F,
    ) -> Result<(), SpawnError>
    where
        F: FnOnce(&mut ThreadManagementState) -> Result<(), SpawnError>,
    {
        match self {
            InnerImpl::Locked(l) => l.spawn_task(task, metrics, on_no_idle),
            InnerImpl::Sharded(s) => s.spawn_task(task, metrics, on_no_idle),
        }
    }

    fn run_worker(
        &self,
        metrics: &SpawnerMetrics,
        keep_alive: Duration,
        worker_thread_id: usize,
    ) -> Option<thread::JoinHandle<()>> {
        match self {
            InnerImpl::Locked(l) => l.run_worker(metrics, keep_alive, worker_thread_id),
            InnerImpl::Sharded(s) => s.run_worker(metrics, keep_alive, worker_thread_id),
        }
    }

    fn begin_shutdown(&self, metrics: &SpawnerMetrics) -> Option<ShutdownHandles> {
        match self {
            InnerImpl::Locked(l) => l.begin_shutdown(),
            InnerImpl::Sharded(s) => s.begin_shutdown(metrics),
        }
    }
}

// This is the original, single-lock implementation of the `spawn_blocking`
// queue. Method scope was principally designed around ensuring that when the
// code was refactored to enable adding a sharded queue implementation, this
// was self-evidently behaviorally identical to the original implementation.
impl LockedImpl {
    fn new(thread_mgmt_state: ThreadManagementState) -> LockedImpl {
        LockedImpl {
            mutex: Mutex::new(LockedInner {
                queue: VecDeque::new(),
                num_notify: 0,
                thread_mgmt_state,
            }),
            condvar: Condvar::new(),
        }
    }

    /// Push a task and either notify an idle worker or invoke
    /// `on_no_idle` (which is responsible for spawning a new worker if
    /// possible).
    fn spawn_task<F>(
        &self,
        task: Task,
        metrics: &SpawnerMetrics,
        on_no_idle: F,
    ) -> Result<(), SpawnError>
    where
        F: FnOnce(&mut ThreadManagementState) -> Result<(), SpawnError>,
    {
        let mut locked = self.mutex.lock();

        if locked.thread_mgmt_state.shutdown {
            drop(locked);

            // Shutdown the task: it's fine to shutdown this task
            // (even if mandatory) because it was scheduled after the
            // shutdown of the runtime began.
            task.task.shutdown();
            return Err(SpawnError::ShuttingDown);
        }

        if metrics.num_idle_threads() == 0 {
            if let Err(e) = on_no_idle(&mut locked.thread_mgmt_state) {
                drop(locked);
                task.shutdown();
                return Err(e);
            }
        } else {
            // Notify an idle worker thread. The notification counter
            // is used to count the needed amount of notifications
            // exactly. Thread libraries may generate spurious
            // wakeups, this counter is used to keep us in a
            // consistent state.
            metrics.dec_num_idle_threads();
            locked.num_notify += 1;
            self.condvar.notify_one();
        }

        locked.queue.push_back(task);
        metrics.inc_queue_depth();

        Ok(())
    }

    /// Run a worker thread's main loop.
    fn run_worker(
        &self,
        metrics: &SpawnerMetrics,
        keep_alive: Duration,
        worker_thread_id: usize,
    ) -> Option<thread::JoinHandle<()>> {
        let mut locked = self.mutex.lock();
        let mut join_on_thread = None;
        // is this thread currently counted in `num_idle_threads`?
        let mut is_counted_idle;

        'main: loop {
            // BUSY
            while let Some(task) = locked.queue.pop_front() {
                metrics.dec_queue_depth();
                drop(locked);
                task.run(metrics);

                locked = self.mutex.lock();
            }

            // IDLE
            metrics.inc_num_idle_threads();
            // mark this thread as currently counted in `num_idle_threads`.
            is_counted_idle = true;

            while !locked.thread_mgmt_state.shutdown {
                let lock_result = self.condvar.wait_timeout(locked, keep_alive).unwrap();

                locked = lock_result.0;
                let timeout_result = lock_result.1;

                if locked.num_notify != 0 {
                    // We have received a legitimate wakeup,
                    // acknowledge it by decrementing the counter
                    // and transition to the BUSY state.
                    locked.num_notify -= 1;
                    // since this is a legitimate wakeup,
                    // the `Spawner::spawn_task` has already
                    // decremented `num_idle_threads`.
                    is_counted_idle = false;
                    break;
                }

                // Even if the condvar "timed out", if the pool is
                // entering the shutdown phase, we want to perform
                // the cleanup logic.
                if !locked.thread_mgmt_state.shutdown && timeout_result.timed_out() {
                    join_on_thread = locked.thread_mgmt_state.worker_timed_out(worker_thread_id);

                    break 'main;
                }

                // Spurious wakeup detected, go back to sleep.
            }

            if locked.thread_mgmt_state.shutdown {
                // Drain the queue
                while let Some(task) = locked.queue.pop_front() {
                    metrics.dec_queue_depth();
                    drop(locked);

                    task.shutdown_or_run_if_mandatory(metrics);

                    locked = self.mutex.lock();
                }

                break;
            }
        }

        // Thread exit
        metrics.dec_num_threads();

        // Is this thread currently counted in `num_idle_threads`?
        if is_counted_idle {
            // `num_idle_threads` should now be tracked exactly,
            // panic with a descriptive message if it is not the
            // case.
            let prev_idle = metrics.dec_num_idle_threads();
            assert_ne!(
                prev_idle, 0,
                "`num_idle_threads` underflowed on thread exit"
            );
        }

        if locked.thread_mgmt_state.shutdown && metrics.num_threads() == 0 {
            self.condvar.notify_one();
        }

        drop(locked);

        join_on_thread
    }

    /// Begin pool shutdown: set the shutdown flag, drop the shutdown
    /// sender, wake all waiting workers, and hand back the worker
    /// `JoinHandle`s for the caller to join.
    fn begin_shutdown(&self) -> Option<ShutdownHandles> {
        let mut locked = self.mutex.lock();
        let handles = locked.thread_mgmt_state.begin_shutdown()?;
        self.condvar.notify_all();
        Some(handles)
    }
}

// Tells whether the error when spawning a thread is temporary.
#[inline]
fn is_temporary_os_thread_error(error: &io::Error) -> bool {
    if matches!(error.kind(), io::ErrorKind::WouldBlock) {
        return true;
    }

    #[cfg(windows)]
    if error.raw_os_error() == Some(1450) {
        // Windows reports temporary resource exhaustion as ERROR_NO_SYSTEM_RESOURCES.
        return true;
    }

    false
}

impl Inner {
    fn run(&self, worker_thread_id: usize) {
        if let Some(f) = &self.after_start {
            f();
        }

        let join_on_thread =
            self.inner_impl
                .run_worker(&self.metrics, self.keep_alive, worker_thread_id);

        if let Some(f) = &self.before_stop {
            f();
        }

        if let Some(handle) = join_on_thread {
            let _ = handle.join();
        }
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("blocking::Spawner").finish()
    }
}
