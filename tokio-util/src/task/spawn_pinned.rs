use std::any::Any;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::{spawn_blocking, spawn_local, JoinHandle, LocalSet};

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
    /// off of the thread. Note that the future is not Send, but the `FnOnce` which
    /// creates it is.
    ///
    /// # Examples
    ///
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
        let worker = self.find_least_burdened_worker();

        // Update task count. Do this before sending the future to ensure that
        // the call to `fetch_sub` does not underflow.
        worker.task_count.fetch_add(1, Ordering::SeqCst);

        // Send the future to the worker
        let (sender, receiver) = channel();
        let task_count = Arc::clone(&worker.task_count);
        let request = FutureRequest {
            func: Box::new(|| {
                Box::new(spawn_local(async move {
                    let result = create_task().await;

                    // Update the task count once the future is finished
                    task_count.fetch_sub(1, Ordering::SeqCst);

                    result
                }))
            }),
            reply: sender,
        };
        worker.spawner.send(request).unwrap();

        // Get the join handle
        let join_handle = receiver.recv().unwrap();
        let join_handle: JoinHandle<Fut::Output> = *join_handle.downcast().unwrap();

        join_handle
    }

    /// Find the worker with the least number of tasks
    fn find_least_burdened_worker(&self) -> &LocalWorkerHandle {
        let (worker, _) = self
            .workers
            .iter()
            .map(|worker| (worker, worker.task_count.load(Ordering::SeqCst)))
            .min_by_key(|&(_, count)| count)
            .expect("There must be more than one worker");
        worker
    }
}

// We need to box the join handle and future spawning closure since they're
// generic and are going through a channel. The join handle will be downcast
// back into the correct type.
type BoxedJoinHandle = Box<dyn Any + Send + 'static>;
type PinnedFutureSpawner = Box<dyn FnOnce() -> BoxedJoinHandle + Send + 'static>;

struct FutureRequest {
    func: PinnedFutureSpawner,
    reply: Sender<BoxedJoinHandle>,
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
        spawn_blocking(|| Self::run(receiver));

        LocalWorkerHandle {
            spawner: sender,
            task_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn run(mut task_receiver: UnboundedReceiver<FutureRequest>) {
        let runtime = Builder::new_current_thread()
            .build()
            .expect("Failed to start a pinned worker thread runtime");

        LocalSet::new().block_on(&runtime, async {
            while let Some(task) = task_receiver.recv().await {
                // Calls spawn_local(future)
                let join_handle = (task.func)();
                task.reply.send(join_handle).unwrap();
            }
        });
    }
}
