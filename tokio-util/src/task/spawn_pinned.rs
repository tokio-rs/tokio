use once_cell::sync::OnceCell;
use std::any::Any;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::{spawn_blocking, spawn_local, JoinHandle, LocalSet};

static PINNED_POOL: OnceCell<PinnedPool> = OnceCell::new();

/// Spawn a task onto a worker thread and pin it there so it can't be moved
/// off of the thread. Note that the future is not Send, but the `FnOnce` which
/// creates it is.
pub fn spawn_pinned<Fut: Future + 'static>(
    create_task: impl FnOnce() -> Fut + Send + 'static,
) -> JoinHandle<Fut::Output>
where
    Fut::Output: Send + 'static,
{
    PinnedPool::handle().spawn_pinned(create_task)
}

struct PinnedPool {
    workers: Vec<PinnedWorkerHandle>,

    /// The index of the next worker to use
    next_worker: AtomicUsize,
}

impl PinnedPool {
    /// Get a reference to the static `PinnedPool`. The pool is created the
    /// first time this is called.
    fn handle() -> &'static PinnedPool {
        PINNED_POOL.get_or_init(|| {
            let worker_count = 8; // FIXME: get this number from somewhere
            let workers = (0..worker_count)
                .map(|_| PinnedWorkerHandle::new_worker())
                .collect();

            PinnedPool {
                workers,
                next_worker: AtomicUsize::new(0),
            }
        })
    }

    /// Spawn a `!Send` future onto a worker
    fn spawn_pinned<Fut: Future + 'static>(
        &self,
        create_task: impl FnOnce() -> Fut + Send + 'static,
    ) -> JoinHandle<Fut::Output>
    where
        Fut::Output: Send + 'static,
    {
        // Choose the worker via round-robin
        let worker_count = self.workers.len();
        let worker_index = self
            .next_worker
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some((x + 1) % worker_count)
            })
            // The closure never returns None
            .unwrap();
        let worker = &self.workers[worker_index];

        // Send the future to the worker
        let (sender, receiver) = channel();
        let request = FutureRequest {
            func: Box::new(|| Box::new(spawn_local(create_task()))),
            reply: sender,
        };
        worker.spawner.send(request).unwrap();

        // Get the join handle
        let join_handle = receiver.recv().unwrap();
        *join_handle.downcast::<JoinHandle<Fut::Output>>().unwrap()
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

impl Debug for FutureRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("FutureRequest")
    }
}

struct PinnedWorkerHandle {
    spawner: UnboundedSender<FutureRequest>,
    // TODO: handle executor shutdown by resetting PinnedPool and workers?
    _thread: JoinHandle<()>,
}

impl PinnedWorkerHandle {
    /// Create a new worker for executing pinned tasks
    fn new_worker() -> PinnedWorkerHandle {
        let (sender, receiver) = unbounded_channel();
        let thread = spawn_blocking(|| Self::run(receiver));

        PinnedWorkerHandle {
            spawner: sender,
            _thread: thread,
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
