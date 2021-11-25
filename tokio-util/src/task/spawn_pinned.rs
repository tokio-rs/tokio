use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::{spawn_local, JoinHandle, LocalSet};

/// A handle to a local pool, used for spawning `!Send` tasks.
#[derive(Clone)]
pub struct LocalPoolHandle {
    pool: Arc<LocalPool>,
}

impl LocalPoolHandle {
    /// Create a new pool of threads to handle `!Send` tasks. Spawn tasks onto this
    /// pool via [`LocalPoolHandle::spawn_pinned`] or
    /// [`LocalPoolHandle::spawn_pinned_blocking`].
    ///
    /// # Panics
    /// Panics if the pool size is less than one.
    pub fn new(pool_size: usize) -> LocalPoolHandle {
        assert!(pool_size > 0);

        let workers = (0..pool_size)
            .map(|_| LocalWorkerHandle::new_worker())
            .collect();

        let pool = Arc::new(LocalPool { workers });

        LocalPoolHandle { pool }
    }

    /// Spawn a task onto a worker thread and pin it there so it can't be moved
    /// off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// This function is async because the `create_task` function must be sent
    /// to a worker thread and spawned there, so we need to wait for the join
    /// handle to be sent back. Alternatively, [`LocalPoolHandle::spawn_pinned_blocking`]
    /// blocks while waiting for the join handle to be send back.
    ///
    /// # Examples
    /// ```
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
    ///         // First await is to get the JoinHandle
    ///         .await // JoinHandle<String>
    ///         // Second await is to get the task output
    ///         .await // Result<String, JoinError>
    ///         .unwrap();
    ///
    ///     assert_eq!(output, "test");
    /// }
    /// ```
    pub async fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.pool.spawn_pinned(create_task).await
    }

    /// Spawn a task onto a worker thread and pin it there so it can't be moved
    /// off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// This is the same as [`LocalPoolHandle::spawn_pinned`], but blocks when
    /// waiting for the join handle.
    ///
    /// # Examples
    /// ```
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
    ///         .spawn_pinned_blocking(|| {
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
    pub fn spawn_pinned_blocking<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        self.pool.spawn_pinned_blocking(create_task)
    }
}

impl Debug for LocalPoolHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("LocalPoolHandle")
    }
}

struct LocalPool {
    workers: Vec<LocalWorkerHandle>,
}

impl LocalPool {
    /// Spawn a `?Send` future onto a worker
    async fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        self.spawn_pinned_inner(create_task, move |join_handle| {
            let _ = sender.send(join_handle);
        });

        receiver.await.expect("Worker failed to send join handle")
    }

    /// Spawn a `?Send` future onto a worker, but block while waiting for the
    /// join handle.
    fn spawn_pinned_blocking<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (sender, receiver) = std::sync::mpsc::channel();

        self.spawn_pinned_inner(create_task, move |join_handle| {
            let _ = sender.send(join_handle);
        });

        receiver.recv().expect("Worker failed to send join handle")
    }

    /// Find a worker and send the task to it. This function is generalized to
    /// work with any type of send back channel.
    fn spawn_pinned_inner<CreateFn, Fut, SendFn>(&self, create_task: CreateFn, send: SendFn)
    where
        CreateFn: FnOnce() -> Fut,
        CreateFn: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
        SendFn: FnOnce(JoinHandle<Fut::Output>),
        SendFn: Send + 'static,
    {
        let worker = self.find_and_incr_least_burdened_worker();
        let task_count = Arc::clone(&worker.task_count);
        let request = FutureRequest {
            spawn: Box::new(move || {
                let join_handle = spawn_local(async move {
                    let result = create_task().await;

                    // Update the task count once the future is finished
                    task_count.fetch_sub(1, Ordering::SeqCst);

                    result
                });

                // Send the join handle back to the spawner.
                send(join_handle);
            }),
        };

        worker
            .spawner
            .send(request)
            .expect("Worker is no longer available");
    }

    /// Find the worker with the least number of tasks, increment its task
    /// count, and return its handle. Make sure to actually spawn a task on
    /// the worker so the task count is kept consistent with load.
    fn find_and_incr_least_burdened_worker(&self) -> &LocalWorkerHandle {
        loop {
            let (worker, task_count) = self
                .workers
                .iter()
                .map(|worker| (worker, worker.task_count.load(Ordering::SeqCst)))
                .min_by_key(|&(_, count)| count)
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
                return worker;
            }
        }
    }
}

type PinnedFutureSpawner = Box<dyn FnOnce() + Send + 'static>;

struct FutureRequest {
    spawn: PinnedFutureSpawner,
}

// Needed for the unwrap in LocalPool::spawn_pinned_inner if sending fails
impl Debug for FutureRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("FutureRequest")
    }
}

struct LocalWorkerHandle {
    spawner: UnboundedSender<FutureRequest>,
    task_count: Arc<AtomicUsize>,
}

impl LocalWorkerHandle {
    /// Create a new worker for executing pinned tasks
    fn new_worker() -> LocalWorkerHandle {
        let (sender, receiver) = unbounded_channel();
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to start a pinned worker thread runtime");

        std::thread::spawn(|| Self::run(runtime, receiver));

        LocalWorkerHandle {
            spawner: sender,
            task_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn run(runtime: tokio::runtime::Runtime, mut task_receiver: UnboundedReceiver<FutureRequest>) {
        LocalSet::new().block_on(&runtime, async {
            while let Some(task) = task_receiver.recv().await {
                // Calls spawn_local(future)
                (task.spawn)();
            }
        });
    }
}
