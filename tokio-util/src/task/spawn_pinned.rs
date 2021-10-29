use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::{spawn_local, JoinHandle, LocalSet};

/// Create a new pool of threads to handle `!Send` tasks. Spawn tasks onto this
/// pool via [`LocalPoolHandle::spawn_pinned`].
pub fn new_local_pool(pool_size: usize) -> LocalPoolHandle {
    assert!(pool_size > 0);

    let workers = (0..pool_size)
        .map(|_| LocalWorkerHandle::new_worker())
        .collect();

    let pool = Arc::new(LocalPool { workers });

    LocalPoolHandle { pool }
}

/// A handle to a local pool created by [`new_local_pool`]
#[derive(Clone)]
pub struct LocalPoolHandle {
    pool: Arc<LocalPool>,
}

impl LocalPoolHandle {
    /// Spawn a task onto a worker thread and pin it there so it can't be moved
    /// off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// # Examples
    /// ```
    /// use std::rc::Rc;
    /// use tokio_util::task::new_local_pool;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Create the local pool
    ///     let pool = new_local_pool(1);
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
    /// ```
    pub fn spawn_pinned<Fut: Future + 'static>(
        &self,
        create_task: impl FnOnce() -> Fut + Send + 'static,
    ) -> JoinHandle<Fut::Output>
    where
        Fut::Output: Send + 'static,
    {
        self.pool.spawn_pinned(create_task)
    }

    /// Spawn a task onto a worker thread and pin it there so it can't be moved
    /// off of the thread. Note that the future is not [`Send`], but the
    /// [`FnOnce`] which creates it is.
    ///
    /// This is the same as [`spawn_pinned`], but is non-blocking and does not
    /// return a join handle.
    ///
    /// # Examples
    /// ```
    /// use std::rc::Rc;
    /// use tokio::sync::mpsc::channel;
    /// use tokio_util::task::new_local_pool;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Create the local pool
    ///     let pool = new_local_pool(1);
    ///     let (sender, mut receiver) = channel(1);
    ///
    ///     // Spawn a !Send future onto the pool
    ///     pool.spawn_pinned_nonblocking(|| {
    ///         // Rc is !Send + !Sync
    ///         let local_data = Rc::new("test");
    ///
    ///         // This future holds an Rc, so it is !Send
    ///         async move { sender.send(*local_data).await.unwrap() }
    ///     });
    ///
    ///     assert_eq!(receiver.recv().await, Some("test"));
    /// }
    /// ```
    pub fn spawn_pinned_nonblocking<Fut: Future<Output = ()> + 'static>(
        &self,
        create_task: impl FnOnce() -> Fut + Send + 'static,
    ) {
        self.pool.spawn_pinned_nonblocking(create_task)
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
    fn spawn_pinned<Fut: Future + 'static>(
        &self,
        create_task: impl FnOnce() -> Fut + Send + 'static,
    ) -> JoinHandle<Fut::Output>
    where
        Fut::Output: Send + 'static,
    {
        let receiver = self.spawn_pinned_inner(create_task);

        // Get the join handle
        receiver.recv().unwrap()
    }

    /// Spawn a `?Send` future onto a worker, when you don't care about the output.
    fn spawn_pinned_nonblocking<Fut: Future<Output = ()> + 'static>(
        &self,
        create_task: impl FnOnce() -> Fut + Send + 'static,
    ) {
        // Don't wait for the join handle
        self.spawn_pinned_inner(create_task);
    }

    fn spawn_pinned_inner<Fut: Future + 'static>(
        &self,
        create_task: impl FnOnce() -> Fut + Send + 'static,
    ) -> std::sync::mpsc::Receiver<JoinHandle<Fut::Output>>
    where
        Fut::Output: Send + 'static,
    {
        let worker = self.find_and_incr_least_burdened_worker();

        // Send the future to the worker
        let (sender, receiver) = channel();
        let task_count = Arc::clone(&worker.task_count);
        let request = FutureRequest {
            spawn: Box::new(move || {
                let join_handle = spawn_local(async move {
                    let result = create_task().await;

                    // Update the task count once the future is finished
                    task_count.fetch_sub(1, Ordering::SeqCst);

                    result
                });

                sender.send(join_handle).ok();
            }),
        };
        worker.spawner.send(request).unwrap();

        receiver
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

            // Make sure the task count hasn't changed when we choose this worker.
            // Otherwise, restart the search.
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

// Needed for the unwrap in LocalPool::spawn_pinned if sending fails
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
        std::thread::spawn(|| Self::run(receiver));

        LocalWorkerHandle {
            spawner: sender,
            task_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn run(mut task_receiver: UnboundedReceiver<FutureRequest>) {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to start a pinned worker thread runtime");

        LocalSet::new().block_on(&runtime, async {
            while let Some(task) = task_receiver.recv().await {
                // Calls spawn_local(future)
                (task.spawn)();
            }
        });
    }
}
