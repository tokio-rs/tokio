//! Multi-threaded runtime

mod handle;
pub(crate) use handle::Handle;

mod idle;
use self::idle::Idle;

mod park;
pub(crate) use park::{Parker, Unparker};

pub(crate) mod queue;

mod worker;
pub(crate) use worker::Launch;

pub(crate) use worker::block_in_place;

use crate::loom::sync::Arc;
use crate::runtime::{blocking, driver, Config, Driver};
use crate::util::RngSeedGenerator;

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub(crate) struct MultiThread {
    handle: Arc<Handle>,
}

// ===== impl MultiThread =====

impl MultiThread {
    pub(crate) fn new(
        size: usize,
        driver: Driver,
        driver_handle: driver::Handle,
        blocking_spawner: blocking::Spawner,
        seed_generator: RngSeedGenerator,
        config: Config,
    ) -> (MultiThread, Launch) {
        let parker = Parker::new(driver);
        let (handle, launch) = worker::create(
            size,
            parker,
            driver_handle,
            blocking_spawner,
            seed_generator,
            config,
        );
        let multi_thread = MultiThread { handle };

        (multi_thread, launch)
    }

    /// Returns reference to `Spawner`.
    ///
    /// The `Spawner` handle can be cloned and enables spawning tasks from other
    /// threads.
    pub(crate) fn handle(&self) -> &Arc<Handle> {
        &self.handle
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
        self.handle.shutdown();
    }
}
