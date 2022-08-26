//! Multi-threaded runtime

mod idle;
use self::idle::Idle;

mod park;
pub(crate) use park::{Parker, Unparker};

pub(crate) mod queue;

mod worker;
pub(crate) use worker::Launch;

pub(crate) use worker::block_in_place;

use crate::loom::sync::Arc;
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::{Config, Driver, HandleInner};

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub(crate) struct MultiThread {
    spawner: Spawner,
}

/// Submits futures to the associated thread pool for execution.
///
/// A `Spawner` instance is a handle to a single thread pool that allows the owner
/// of the handle to spawn futures onto the thread pool.
///
/// The `Spawner` handle is *only* used for spawning new futures. It does not
/// impact the lifecycle of the thread pool in any way. The thread pool may
/// shut down while there are outstanding `Spawner` instances.
///
/// `Spawner` instances are obtained by calling [`MultiThread::spawner`].
///
/// [`MultiThread::spawner`]: method@MultiThread::spawner
#[derive(Clone)]
pub(crate) struct Spawner {
    shared: Arc<worker::Shared>,
}

// ===== impl MultiThread =====

impl MultiThread {
    pub(crate) fn new(
        size: usize,
        driver: Driver,
        handle_inner: HandleInner,
        config: Config,
    ) -> (MultiThread, Launch) {
        let parker = Parker::new(driver);
        let (shared, launch) = worker::create(size, parker, handle_inner, config);
        let spawner = Spawner { shared };
        let multi_thread = MultiThread { spawner };

        (multi_thread, launch)
    }

    /// Returns reference to `Spawner`.
    ///
    /// The `Spawner` handle can be cloned and enables spawning tasks from other
    /// threads.
    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    /// Blocks the current thread waiting for the future to complete.
    ///
    /// The future will execute on the current thread, but all spawned tasks
    /// will be executed on the thread pool.
    pub(crate) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let mut enter = crate::runtime::enter(true);
        enter.block_on(future).expect("failed to park thread")
    }
}

impl fmt::Debug for MultiThread {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("MultiThread").finish()
    }
}

impl Drop for MultiThread {
    fn drop(&mut self) {
        self.spawner.shutdown();
    }
}

// ==== impl Spawner =====

impl Spawner {
    /// Spawns a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F, id: task::Id) -> JoinHandle<F::Output>
    where
        F: crate::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        worker::Shared::bind_new_task(&self.shared, future, id)
    }

    pub(crate) fn shutdown(&mut self) {
        self.shared.close();
    }

    pub(crate) fn as_handle_inner(&self) -> &HandleInner {
        self.shared.as_handle_inner()
    }
}

cfg_metrics! {
    use crate::runtime::{SchedulerMetrics, WorkerMetrics};

    impl Spawner {
        pub(crate) fn num_workers(&self) -> usize {
            self.shared.worker_metrics.len()
        }

        pub(crate) fn scheduler_metrics(&self) -> &SchedulerMetrics {
            &self.shared.scheduler_metrics
        }

        pub(crate) fn worker_metrics(&self, worker: usize) -> &WorkerMetrics {
            &self.shared.worker_metrics[worker]
        }

        pub(crate) fn injection_queue_depth(&self) -> usize {
            self.shared.injection_queue_depth()
        }

        pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
            self.shared.worker_local_queue_depth(worker)
        }
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}
