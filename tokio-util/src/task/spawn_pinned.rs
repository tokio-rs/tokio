use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
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
        let (sender, receiver) = oneshot::channel();

        let worker = self.find_and_incr_least_burdened_worker();
        let job_guard = JobGuard(Arc::clone(&worker.task_count));
        let worker_spawner = worker.spawner.clone();

        // Spawn a future onto the worker's runtime so can immediately return
        // a join handle.
        worker.runtime_handle.spawn(async move {
            // Move the job guard into the task
            let _job_guard = job_guard;

            // Inside the future we can't run spawn_local yet because we're not
            // in the context of a LocalSet. We need to send create_task to the
            // LocalSet task for spawning.
            let spawn_task = Box::new(move || {
                // Once we're in the LocalSet context we can call spawn_local
                let join_handle = spawn_local(async move { create_task().await });

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

            // Wait for the task's join handle. Forward task cancellation in
            // case this task gets canceled (via ReceiverCancelGuard).
            let join_handle = match ReceiverCancelGuard(receiver).await {
                Ok(handle) => handle,
                Err(e) => {
                    // We sent the task successfully, but failed to get its
                    // join handle... We assume something happened to the worker
                    // and the task was not spawned. Propagate the error as a
                    // panic in the join handle.
                    panic!("Worker failed to send join handle: {}", e);
                }
            };

            // Wait for the task to complete. Forward task cancellation in case
            // this task gets canceled.
            let join_result = JoinHandleCancelGuard(join_handle).await;

            match join_result {
                Ok(output) => output,
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

/// Automatically decrements a worker's job count when a job finishes (when
/// this gets dropped).
struct JobGuard(Arc<AtomicUsize>);

impl Drop for JobGuard {
    fn drop(&mut self) {
        // Decrement the job count
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Automatically abort/cancel the task when this guard gets dropped. This will
/// forward a cancellation from one task to another.
///
/// This implements Future by polling the join handle, so just await it.
struct JoinHandleCancelGuard<T>(JoinHandle<T>);

impl<T> Future for JoinHandleCancelGuard<T> {
    type Output = <JoinHandle<T> as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let join_handle = Pin::new(&mut self.0);
        join_handle.poll(cx)
    }
}

impl<T> Drop for JoinHandleCancelGuard<T> {
    fn drop(&mut self) {
        // Attempt to abort the task. This does nothing if the task has already
        // completed.
        self.0.abort();
    }
}

/// If the task is canceled while waiting for the join handle, this guard will
/// check if the join handle was sent (in-transit so it wasn't aborted on the
/// worker side) and abort it if so.
struct ReceiverCancelGuard<T>(oneshot::Receiver<JoinHandle<T>>);

impl<T> Future for ReceiverCancelGuard<T> {
    type Output = <oneshot::Receiver<JoinHandle<T>> as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let receiver = Pin::new(&mut self.0);
        receiver.poll(cx)
    }
}

impl<T> Drop for ReceiverCancelGuard<T> {
    fn drop(&mut self) {
        // If task is canceled while waiting for the join handle, and the join
        // handle was already "sent" by the worker, then it's in a limbo state
        // and needs to be manually canceled here.
        if let Ok(join_handle) = self.0.try_recv() {
            join_handle.abort();
        }
    }
}

type PinnedFutureSpawner = Box<dyn FnOnce() + Send + 'static>;

struct LocalWorkerHandle {
    runtime_handle: tokio::runtime::Handle,
    spawner: UnboundedSender<PinnedFutureSpawner>,
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

    fn run(
        runtime: tokio::runtime::Runtime,
        mut task_receiver: UnboundedReceiver<PinnedFutureSpawner>,
    ) {
        LocalSet::new().block_on(&runtime, async {
            while let Some(spawn_task) = task_receiver.recv().await {
                // Calls spawn_local(future)
                (spawn_task)();
            }
        });
    }
}
