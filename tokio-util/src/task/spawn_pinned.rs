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
    /// pool via [`LocalPoolHandle::spawn_pinned`].
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
    ///         .await
    ///         .unwrap();
    ///
    ///     assert_eq!(output, "test");
    /// }
    /// ```
    pub fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
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
    fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut,
        F: Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        let worker = self.find_and_incr_least_burdened_worker();
        let worker_handle_clone = worker.clone();

        // Spawn a future onto the worker's runtime so can immediately return
        // a join handle.
        worker.runtime_handle.spawn(async move {
            // Inside the future we can't run spawn_local yet because we're not
            // in the context of a LocalSet. We need to send create_task to the
            // LocalSet task for spawning.
            let request = FutureRequest {
                spawn: Box::new(move || {
                    // Once we're in the LocalSet context we can call spawn_local
                    let join_handle = spawn_local(create_task());

                    // Send the join handle back to the spawner.
                    let _ = sender.send(join_handle);
                }),
            };

            // Send the future request to the LocalSet task
            if let Err(e) = worker_handle_clone.spawner.send(request) {
                // Failed to spawn the task on the worker, so we need to remove
                // the task from the count.
                worker_handle_clone.decrement_task_count();

                // TODO: Recreate worker? Otherwise future spawn_pinned calls
                //       might fail if they try to use this worker.

                // Propagate the error as a panic in the join handle.
                panic!("Worker is no longer available: {}", e);
            }

            // Wait for the task's join handle
            let join_handle = match receiver.await {
                Ok(handle) => handle,
                Err(e) => {
                    // We sent the task successfully, but failed to get its
                    // join handle... We create_task panicked and the task was
                    // not spawned, so we need to remove the task from the count.
                    worker_handle_clone.decrement_task_count();

                    // TODO: Recreate worker? Otherwise future spawn_pinned calls
                    //       might fail if they try to use this worker.

                    // Propagate the error as a panic in the join handle.
                    panic!("Worker failed to send join handle: {}", e);
                }
            };

            // Wait for the task to complete
            let join_result = join_handle.await;

            // Update the task count once the future has finished
            worker_handle_clone.decrement_task_count();

            match join_result {
                Ok(output) => output,
                Err(e) => {
                    // Task panicked or was canceled. Forward this error as a
                    // panic in the join handle.
                    panic!("spawn_pinned task panicked or aborted: {}", e);
                }
            }
        })
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

#[derive(Clone)]
struct LocalWorkerHandle {
    runtime_handle: tokio::runtime::Handle,
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
        let runtime_handle = runtime.handle().clone();

        std::thread::spawn(|| Self::run(runtime, receiver));

        LocalWorkerHandle {
            runtime_handle,
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

    fn decrement_task_count(&self) {
        self.task_count.fetch_sub(1, Ordering::SeqCst);
    }
}
