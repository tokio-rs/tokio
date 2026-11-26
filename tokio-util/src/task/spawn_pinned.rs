use pin_project_lite::pin_project;
use std::cell::Cell;
use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// A cloneable handle to a local pool, used for spawning `!Send` tasks.
///
/// Internally the local pool uses a [`tokio::runtime::LocalRuntime`] for each worker thread
/// in the pool. Consequently you can also use [`tokio::task::spawn_local`] (which will
/// execute on the same thread) inside the Future you supply to the various spawn methods
/// of `LocalPoolHandle`.
///
/// [`tokio::runtime::LocalRuntime`]: tokio::runtime::LocalRuntime
/// [`tokio::task::spawn_local`]: tokio::task::spawn_local
///
/// # Examples
///
/// ```
/// # #[cfg(not(target_family = "wasm"))]
/// # {
/// use std::rc::Rc;
/// use tokio::task;
/// use tokio_util::task::LocalPoolHandle;
///
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() {
///     let pool = LocalPoolHandle::new(5);
///
///     let output = pool.spawn_pinned(|| {
///         // `data` is !Send + !Sync
///         let data = Rc::new("local data");
///         let data_clone = data.clone();
///
///         async move {
///             task::spawn_local(async move {
///                 println!("{}", data_clone);
///             });
///
///             data.to_string()
///         }
///     }).await.unwrap();
///     println!("output: {}", output);
/// }
/// # }
/// ```
///
#[derive(Clone)]
pub struct LocalPoolHandle {
    pool: Arc<LocalPool>,
}

impl LocalPoolHandle {
    /// Create a new pool of threads to handle `!Send` tasks. Spawn tasks onto this
    /// pool via [`LocalPoolHandle::spawn_pinned`].
    ///
    /// # Panics
    ///
    /// Panics if the pool size is less than one.
    #[track_caller]
    pub fn new(pool_size: usize) -> LocalPoolHandle {
        assert!(pool_size > 0);

        let workers = (0..pool_size)
            .map(|_| LocalWorkerHandle::new_worker())
            .collect();

        let pool = Arc::new(LocalPool { workers });

        LocalPoolHandle { pool }
    }

    /// Returns the number of threads of the Pool.
    #[inline]
    pub fn num_threads(&self) -> usize {
        self.pool.workers.len()
    }

    /// Returns the number of tasks scheduled on each worker. The indices of the
    /// worker threads correspond to the indices of the returned `Vec`.
    pub fn get_task_loads_for_each_worker(&self) -> Vec<usize> {
        self.pool
            .workers
            .iter()
            .map(|worker| worker.shared.task_count.load(Ordering::SeqCst))
            .collect::<Vec<_>>()
    }

    /// Spawn a task onto a worker thread and pin it there so it can't be moved
    /// off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// # Examples
    /// ```
    /// # #[cfg(not(target_family = "wasm"))]
    /// # {
    /// use std::rc::Rc;
    /// use tokio_util::task::LocalPoolHandle;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Create the local pool
    ///     let pool = LocalPoolHandle::new(1);
    ///
    ///     // Spawn a !Send future onto the pool and await it
    ///     let output = pool
    ///         .spawn_pinned(|| {
    ///             // Rc is !Send + !Sync
    ///             let local_data = Rc::new("test");
    ///
    ///             // This future holds an Rc, so it is !Send
    ///             async move { local_data.to_string() }
    ///         })
    ///         .await
    ///         .unwrap();
    ///
    ///     assert_eq!(output, "test");
    /// }
    /// # }
    /// ```
    #[track_caller]
    pub fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.pool
            .spawn_pinned(create_task, WorkerChoice::LeastBurdened)
    }

    /// Differs from `spawn_pinned` only in that you can choose a specific worker thread
    /// of the pool, whereas `spawn_pinned` chooses the worker with the smallest
    /// number of tasks scheduled.
    ///
    /// A worker thread is chosen by index. Indices are 0 based and the largest index
    /// is given by `num_threads() - 1`
    ///
    /// # Panics
    ///
    /// This method panics if the index is out of bounds.
    ///
    /// # Examples
    ///
    /// This method can be used to spawn a task on all worker threads of the pool:
    ///
    /// ```
    /// # #[cfg(not(target_family = "wasm"))]
    /// # {
    /// use tokio_util::task::LocalPoolHandle;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     const NUM_WORKERS: usize = 3;
    ///     let pool = LocalPoolHandle::new(NUM_WORKERS);
    ///     let handles = (0..pool.num_threads())
    ///         .map(|worker_idx| {
    ///             pool.spawn_pinned_by_idx(
    ///                 || {
    ///                     async {
    ///                         "test"
    ///                     }
    ///                 },
    ///                 worker_idx,
    ///             )
    ///         })
    ///         .collect::<Vec<_>>();
    ///
    ///     for handle in handles {
    ///         handle.await.unwrap();
    ///     }
    /// }
    /// # }
    /// ```
    ///
    #[track_caller]
    pub fn spawn_pinned_by_idx<F, Fut>(&self, create_task: F, idx: usize) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.pool
            .spawn_pinned(create_task, WorkerChoice::ByIdx(idx))
    }
}

impl Debug for LocalPoolHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("LocalPoolHandle")
    }
}

enum WorkerChoice {
    LeastBurdened,
    ByIdx(usize),
}

struct LocalPool {
    workers: Box<[LocalWorkerHandle]>,
}

impl LocalPool {
    /// Spawn a `?Send` future onto a worker
    #[track_caller]
    fn spawn_pinned<F, Fut>(
        &self,
        create_task: F,
        worker_choice: WorkerChoice,
    ) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (worker, job_guard) = match worker_choice {
            WorkerChoice::LeastBurdened => self.find_and_incr_least_burdened_worker(),
            WorkerChoice::ByIdx(idx) => self.find_worker_by_idx(idx),
        };

        worker
            .runtime_handle
            .spawn(PinnedFuture::new(create_task, job_guard))
    }

    /// Find the worker with the least number of tasks, increment its task
    /// count, and return its handle. Make sure to actually spawn a task on
    /// the worker so the task count is kept consistent with load.
    ///
    /// A job count guard is also returned to ensure the task count gets
    /// decremented when the job is done.
    fn find_and_incr_least_burdened_worker(&self) -> (&LocalWorkerHandle, JobCountGuard) {
        if self.workers.len() == 1 {
            let worker = &self.workers[0];
            worker.shared.task_count.fetch_add(1, Ordering::SeqCst);
            return (worker, JobCountGuard(Arc::clone(&worker.shared)));
        }

        loop {
            let (worker, mut task_count) = self
                .workers
                .iter()
                .map(|worker| (worker, worker.shared.task_count.load(Ordering::Relaxed)))
                .min_by_key(|&(_, count)| count)
                .expect("There must be more than one worker");

            // Make sure the task count hasn't increased since when we chose
            // this worker. If it decreased (or on a spurious failure), retry
            // on the same worker without rescanning all workers.
            loop {
                match worker.shared.task_count.compare_exchange_weak(
                    task_count,
                    task_count + 1,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return (worker, JobCountGuard(Arc::clone(&worker.shared))),
                    Err(actual) if actual <= task_count => task_count = actual,
                    Err(_) => break,
                }
            }
        }
    }

    #[track_caller]
    fn find_worker_by_idx(&self, idx: usize) -> (&LocalWorkerHandle, JobCountGuard) {
        let worker = &self.workers[idx];
        worker.shared.task_count.fetch_add(1, Ordering::SeqCst);

        (worker, JobCountGuard(Arc::clone(&worker.shared)))
    }
}

thread_local! {
    // Points to the `WorkerShared` of the worker currently running on this
    // thread, or null if this thread is not executing a local pool worker.
    static CURRENT_WORKER: Cell<*const ()> = const { Cell::new(std::ptr::null()) };
}

struct CurrentWorkerGuard;

impl CurrentWorkerGuard {
    fn enter(worker: &Arc<WorkerShared>) -> Self {
        let ptr = Arc::as_ptr(worker).cast::<()>();
        CURRENT_WORKER.with(|cell| {
            debug_assert!(cell.get().is_null());
            cell.set(ptr);
        });
        Self
    }
}

impl Drop for CurrentWorkerGuard {
    fn drop(&mut self) {
        let _ = CURRENT_WORKER.try_with(|cell| cell.set(std::ptr::null()));
    }
}

#[repr(align(128))]
struct WorkerShared {
    task_count: AtomicUsize,
    closed: AtomicBool,
    shutdown_notify: Notify,
}

impl WorkerShared {
    fn is_current_worker(self: &Arc<Self>) -> bool {
        let expected = Arc::as_ptr(self).cast::<()>();
        CURRENT_WORKER
            .try_with(|cell| std::ptr::eq(cell.get(), expected))
            .unwrap_or(false)
    }
}

/// Automatically decrements a worker's job count when a job finishes (when
/// this gets dropped), and signals worker shutdown if the pool has been
/// dropped and no tasks remain.
struct JobCountGuard(Arc<WorkerShared>);

impl Drop for JobCountGuard {
    fn drop(&mut self) {
        // Decrement the job count
        let previous_value = self.0.task_count.fetch_sub(1, Ordering::SeqCst);
        debug_assert!(previous_value >= 1);
        if previous_value == 1 && self.0.closed.load(Ordering::SeqCst) {
            self.0.shutdown_notify.notify_one();
        }
    }
}

pin_project! {
    struct PinnedFuture<F, Fut> {
        #[pin]
        state: PinnedFutureState<F, Fut>,
        job_guard: JobCountGuard,
    }
    impl<F, Fut> PinnedDrop for PinnedFuture<F, Fut> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            // If `state` currently holds the `!Send` future, verify that we
            // are on the worker thread that created it before its destructor
            // runs.
            if matches!(
                this.state.as_ref().project_ref(),
                PinnedFutureStateProjRef::Running { .. }
            ) && !this.job_guard.0.is_current_worker()
            {
                std::process::abort();
            }
        }
    }
}

pin_project! {
    #[project = PinnedFutureStateProj]
    #[project_ref = PinnedFutureStateProjRef]
    #[project_replace = PinnedFutureStateProjReplace]
    enum PinnedFutureState<F, Fut> {
        Init { create_task: F },
        Running { #[pin] fut: Fut },
        Done,
    }
}

// SAFETY: `PinnedFuture` starts in `Init`, which contains only `F: Send` and
// `JobCountGuard: Send`. The `!Send` future `Fut` is only created and polled
// inside `Future::poll` after asserting `job_guard.0.is_current_worker()`.
// Because `job_guard` owns an `Arc<WorkerShared>`, the worker's identity
// pointer stays uniquely allocated for the lifetime of `PinnedFuture` and is
// only installed in `CURRENT_WORKER` on that worker's dedicated thread while
// driving or dropping its `LocalRuntime`. `PinnedDrop` checks the same thread
// identity before `Running { fut }` can be dropped and aborts on a mismatch,
// ensuring `Fut` is never created, accessed, polled, or dropped on any other
// thread.
unsafe impl<F: Send, Fut> Send for PinnedFuture<F, Fut> {}

impl<F, Fut> PinnedFuture<F, Fut> {
    fn new(create_task: F, job_guard: JobCountGuard) -> Self {
        Self {
            state: PinnedFutureState::Init { create_task },
            job_guard,
        }
    }
}

impl<F, Fut> Future for PinnedFuture<F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future,
{
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        assert!(
            this.job_guard.0.is_current_worker(),
            "pinned task polled on wrong thread"
        );

        let mut state = this.state;

        if matches!(
            state.as_ref().project_ref(),
            PinnedFutureStateProjRef::Init { .. }
        ) {
            // Transition to `Done` before calling `create_task` so a panic in
            // the callback leaves no future behind.
            let create_task = match state.as_mut().project_replace(PinnedFutureState::Done) {
                PinnedFutureStateProjReplace::Init { create_task } => create_task,
                _ => unreachable!(),
            };
            let fut = create_task();
            state.set(PinnedFutureState::Running { fut });
        }

        match state.as_mut().project() {
            PinnedFutureStateProj::Running { fut } => match fut.poll(cx) {
                Poll::Ready(output) => {
                    // Drop the `!Send` future immediately on the worker thread.
                    state.set(PinnedFutureState::Done);
                    Poll::Ready(output)
                }
                Poll::Pending => Poll::Pending,
            },
            PinnedFutureStateProj::Init { .. } => unreachable!(),
            PinnedFutureStateProj::Done => {
                panic!("pinned future polled after completion")
            }
        }
    }
}

struct LocalWorkerHandle {
    runtime_handle: tokio::runtime::Handle,
    shared: Arc<WorkerShared>,
}

impl Drop for LocalWorkerHandle {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::SeqCst);
        if self.shared.task_count.load(Ordering::SeqCst) == 0 {
            self.shared.shutdown_notify.notify_one();
        }
    }
}

impl LocalWorkerHandle {
    /// Create a new worker for executing pinned tasks
    fn new_worker() -> LocalWorkerHandle {
        let (handle_sender, handle_receiver) = std::sync::mpsc::channel();
        let shared = Arc::new(WorkerShared {
            task_count: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            shutdown_notify: Notify::new(),
        });
        let shared_clone = Arc::clone(&shared);

        std::thread::spawn(|| Self::run(handle_sender, shared_clone));

        let runtime_handle = handle_receiver
            .recv()
            .expect("Failed to recv local runtime init result")
            .expect("Failed to start local runtime");

        LocalWorkerHandle {
            runtime_handle,
            shared,
        }
    }

    fn run(
        handle_sender: std::sync::mpsc::Sender<std::io::Result<tokio::runtime::Handle>>,
        shared: Arc<WorkerShared>,
    ) {
        // Declared before `runtime` so `CURRENT_WORKER` remains set while
        // `runtime` drops any remaining tasks during shutdown or unwinding.
        let _worker_guard = CurrentWorkerGuard::enter(&shared);

        let runtime = match tokio::runtime::LocalRuntime::new() {
            Ok(runtime) => runtime,
            Err(err) => {
                let _ = handle_sender.send(Err(err));
                return;
            }
        };

        let runtime_handle = runtime.handle().clone();

        handle_sender
            .send(Ok(runtime_handle))
            .expect("Failed to send local runtime handle");
        drop(handle_sender);

        runtime.block_on(async {
            loop {
                let notified = shared.shutdown_notify.notified();
                tokio::pin!(notified);
                // Register before checking the shutdown condition so a
                // notification sent after the check cannot be missed.
                notified.as_mut().enable();

                if shared.closed.load(Ordering::SeqCst)
                    && shared.task_count.load(Ordering::SeqCst) == 0
                {
                    break;
                }

                notified.await;
            }
        });

        // All `spawn_pinned` tasks have already completed and dropped their
        // `JobCountGuard` before `block_on` exited. Drop the local runtime on
        // this worker thread so any detached `spawn_local` tasks are cancelled
        // cleanly.
        drop(runtime);
    }
}
