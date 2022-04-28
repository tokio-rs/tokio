use futures_util::future::{AbortHandle, Abortable};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::task::{spawn_local, JoinHandle, LocalSet};

/// A handle to a local pool, used for spawning `!Send` tasks.
#[derive(Clone)]
pub struct LocalPoolHandle<T: Clone + Default> {
    pool: Arc<LocalPool<T>>,
}

impl<T: Clone + Default + 'static> LocalPoolHandle<T> {
    /// Create a new pool of threads to handle `!Send` tasks. Spawn tasks onto this
    /// pool via [`LocalPoolHandle::spawn_pinned`].
    ///
    /// Each worker thread can have a local `!Send` data that tasks running on a
    /// same worker thread can mutually access without synchronization. A local
    /// data is initialized with [`Default`].
    ///
    /// # Examples
    /// ```
    /// use std::cell::RefCell;
    /// use std::rc::Rc;
    /// use tokio_util::task::LocalPoolHandle;
    ///
    /// LocalPoolHandle::<Rc<RefCell<String>>>::new(1);
    /// ```
    ///
    /// # Panics
    /// Panics if the pool size is less than one.
    pub fn new(pool_size: usize) -> LocalPoolHandle<T> {
        assert!(pool_size > 0);

        let workers = (0..pool_size)
            .map(|_| LocalWorkerHandle::new_worker())
            .collect();

        let pool = Arc::new(LocalPool { workers });

        LocalPoolHandle { pool }
    }

    /// Spawn a task onto a least burdened worker thread and pin it there so it can't be moved
    /// off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// # Examples
    /// ```
    /// use std::rc::Rc;
    /// use tokio_util::task::LocalPoolHandle;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Create the local pool
    ///     let pool = LocalPoolHandle::<()>::new(1);
    ///
    ///     // Spawn a !Send future onto the pool and await it
    ///     let output = pool
    ///         .spawn_pinned(|_| {
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
    /// ```
    pub fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(T) -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.pool.spawn_pinned(create_task)
    }

    /// Spawn a task onto a specific worker thread and pin it there so it can't
    /// be moved off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// # Examples
    /// ```
    /// use std::cell::RefCell;
    /// use std::rc::Rc;
    /// use tokio_util::task::LocalPoolHandle;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Create the local pool with a local !Send data
    ///     let pool = LocalPoolHandle::<Rc<RefCell<String>>>::new(2);
    ///
    ///     pool
    ///         .spawn_pinned_at(0, |data| async move {
    ///             *data.borrow_mut() = "test".to_string();
    ///         })
    ///         .await
    ///         .unwrap();
    ///
    ///     // Spawn a task onto the same worker thread
    ///     let output = pool
    ///         .spawn_pinned_at(0, |data| async move {
    ///             data.borrow().clone()
    ///         })
    ///         .await
    ///         .unwrap();
    ///
    ///     assert_eq!(output, "test");
    /// }
    /// ```
    ///
    /// # Panics
    /// Panics if the worker index is out of range.
    pub fn spawn_pinned_at<F, Fut>(&self, worker_index: usize, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(T) -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.pool.spawn_pinned_at(worker_index, create_task)
    }
}

impl<T: Clone + Default> Debug for LocalPoolHandle<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("LocalPoolHandle")
    }
}

struct LocalPool<T: Clone + Default> {
    workers: Vec<LocalWorkerHandle<T>>,
}

impl<T: Clone + Default + 'static> LocalPool<T> {
    /// Spawn a `?Send` future onto a worker
    fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(T) -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (worker_index, job_guard) = self.find_and_incr_least_burdened_worker();
        self.do_spawn_pinned(worker_index, job_guard, create_task)
    }

    /// Spawn a `?Send` future onto a specified worker
    fn spawn_pinned_at<F, Fut>(&self, worker_index: usize, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(T) -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        if worker_index >= self.workers.len() {
            panic!("the number of workers is {} but the index is {}", self.workers.len(), worker_index);
        }

        let job_guard = self.incr_worker(worker_index);
        self.do_spawn_pinned(worker_index, job_guard, create_task)
    }

    fn do_spawn_pinned<F, Fut>(&self, worker_index: usize, job_guard: JobCountGuard, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(T) -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();

        let worker = &self.workers[worker_index];
        let worker_spawner = worker.spawner.clone();

        // Spawn a future onto the worker's runtime so we can immediately return
        // a join handle.
        worker.runtime_handle.spawn(async move {
            // Move the job guard into the task
            let _job_guard = job_guard;

            // Propagate aborts via Abortable/AbortHandle
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let _abort_guard = AbortGuard(abort_handle);

            // Inside the future we can't run spawn_local yet because we're not
            // in the context of a LocalSet. We need to send create_task to the
            // LocalSet task for spawning.
            let spawn_task = Box::new(move |data: T| {
                // Once we're in the LocalSet context we can call spawn_local
                let join_handle =
                    spawn_local(
                        async move { Abortable::new(create_task(data), abort_registration).await },
                    );

                // Send the join handle back to the spawner. If sending fails,
                // we assume the parent task was canceled, so cancel this task
                // as well.
                if let Err(join_handle) = sender.send(join_handle) {
                    join_handle.abort()
                }
            });

            // Send the callback to the LocalSet task
            if let Err(e) = worker_spawner.send(spawn_task) {
                // Propagate the error as a panic in the join handle.
                panic!("Failed to send job to worker: {}", e);
            }

            // Wait for the task's join handle
            let join_handle = match receiver.await {
                Ok(handle) => handle,
                Err(e) => {
                    // We sent the task successfully, but failed to get its
                    // join handle... We assume something happened to the worker
                    // and the task was not spawned. Propagate the error as a
                    // panic in the join handle.
                    panic!("Worker failed to send join handle: {}", e);
                }
            };

            // Wait for the task to complete
            let join_result = join_handle.await;

            match join_result {
                Ok(Ok(output)) => output,
                Ok(Err(_)) => {
                    // Pinned task was aborted. But that only happens if this
                    // task is aborted. So this is an impossible branch.
                    unreachable!(
                        "Reaching this branch means this task was previously \
                         aborted but it continued running anyways"
                    )
                }
                Err(e) => {
                    if e.is_panic() {
                        std::panic::resume_unwind(e.into_panic());
                    } else if e.is_cancelled() {
                        // No one else should have the join handle, so this is
                        // unexpected. Forward this error as a panic in the join
                        // handle.
                        panic!("spawn_pinned task was canceled: {}", e);
                    } else {
                        // Something unknown happened (not a panic or
                        // cancellation). Forward this error as a panic in the
                        // join handle.
                        panic!("spawn_pinned task failed: {}", e);
                    }
                }
            }
        })
    }

    /// Find the worker with the least number of tasks, increment its task
    /// count, and return its handle. Make sure to actually spawn a task on
    /// the worker so the task count is kept consistent with load.
    ///
    /// A job count guard is also returned to ensure the task count gets
    /// decremented when the job is done.
    fn find_and_incr_least_burdened_worker(&self) -> (usize, JobCountGuard) {
        loop {
            let (worker_index, worker, task_count) = self
                .workers
                .iter()
                .enumerate()
                .map(|(i, worker)| (i, worker, worker.task_count.load(Ordering::SeqCst)))
                .min_by_key(|&(_, _, count)| count)
                .expect("There must be more than one worker");

            // Make sure the task count hasn't changed since when we choose this
            // worker. Otherwise, restart the search.
            if worker
                .task_count
                .compare_exchange(
                    task_count,
                    task_count + 1,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return (worker_index, JobCountGuard(Arc::clone(&worker.task_count)));
            }
        }
    }

    fn incr_worker(&self, worker_index: usize) -> JobCountGuard {
        let task_count = &self.workers[worker_index].task_count;
        task_count.fetch_add(1, Ordering::SeqCst);
        JobCountGuard(Arc::clone(task_count))
    }
}

/// Automatically decrements a worker's job count when a job finishes (when
/// this gets dropped).
struct JobCountGuard(Arc<AtomicUsize>);

impl Drop for JobCountGuard {
    fn drop(&mut self) {
        // Decrement the job count
        let previous_value = self.0.fetch_sub(1, Ordering::SeqCst);
        debug_assert!(previous_value >= 1);
    }
}

/// Calls abort on the handle when dropped.
struct AbortGuard(AbortHandle);

impl Drop for AbortGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type PinnedFutureSpawner<T> = Box<dyn FnOnce(T) + Send + 'static>;

struct LocalWorkerHandle<T: Clone + Default> {
    runtime_handle: tokio::runtime::Handle,
    spawner: UnboundedSender<PinnedFutureSpawner<T>>,
    task_count: Arc<AtomicUsize>,
}

impl<T: Clone + Default + 'static> LocalWorkerHandle<T> {
    /// Create a new worker for executing pinned tasks
    fn new_worker() -> Self {
        let (sender, receiver) = unbounded_channel();
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to start a pinned worker thread runtime");
        let runtime_handle = runtime.handle().clone();
        let task_count = Arc::new(AtomicUsize::new(0));
        let task_count_clone = Arc::clone(&task_count);

        std::thread::spawn(|| Self::run(runtime, receiver, task_count_clone));

        Self {
            runtime_handle,
            spawner: sender,
            task_count,
        }
    }

    fn run(
        runtime: tokio::runtime::Runtime,
        mut task_receiver: UnboundedReceiver<PinnedFutureSpawner<T>>,
        task_count: Arc<AtomicUsize>,
    ) {
        let local_set = LocalSet::new();
        local_set.block_on(&runtime, async {
            let data = T::default();
            while let Some(spawn_task) = task_receiver.recv().await {
                // Calls spawn_local(future)
                (spawn_task)(data.clone());
            }
        });

        // If there are any tasks on the runtime associated with a LocalSet task
        // that has already completed, but whose output has not yet been
        // reported, let that task complete.
        //
        // Since the task_count is decremented when the runtime task exits,
        // reading that counter lets us know if any such tasks completed during
        // the call to `block_on`.
        //
        // Tasks on the LocalSet can't complete during this loop since they're
        // stored on the LocalSet and we aren't accessing it.
        let mut previous_task_count = task_count.load(Ordering::SeqCst);
        loop {
            // This call will also run tasks spawned on the runtime.
            runtime.block_on(tokio::task::yield_now());
            let new_task_count = task_count.load(Ordering::SeqCst);
            if new_task_count == previous_task_count {
                break;
            } else {
                previous_task_count = new_task_count;
            }
        }

        // It's now no longer possible for a task on the runtime to be
        // associated with a LocalSet task that has completed. Drop both the
        // LocalSet and runtime to let tasks on the runtime be cancelled if and
        // only if they are still on the LocalSet.
        //
        // Drop the LocalSet task first so that anyone awaiting the runtime
        // JoinHandle will see the cancelled error after the LocalSet task
        // destructor has completed.
        drop(local_set);
        drop(runtime);
    }
}
