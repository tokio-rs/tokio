//! Implementer notes:
//! Right now, the plan is to mirror `basic_scheduler`. We can then intercept
//! calls to spawn and inject a task ID. Further, we're able to drive the runtime
//! and the `Syscalls` trait at the same time.
use crate::runtime::BasicScheduler;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
mod park;
use crate::syscall::Syscalls;
use crate::task::JoinHandle;

/// Deterministically executes tasks on the current thread,
pub(crate) struct TestScheduler {
    inner: BasicScheduler<park::SyscallsPark>,
    spawner: Spawner,
    syscalls: Arc<dyn Syscalls>,
}

#[derive(Clone)]
pub(crate) struct Spawner {
    inner: crate::runtime::basic_scheduler::Spawner,
    syscalls: Arc<dyn Syscalls>,
}


impl TestScheduler {
    pub(crate) fn new(syscalls: Arc<dyn Syscalls>) -> Self {
        let park = park::SyscallsPark::new(Arc::clone(&syscalls));
        let inner = BasicScheduler::new(park);
        let spawner = Spawner {
            inner: inner.spawner().clone(),
            syscalls: Arc::clone(&syscalls)
        };
        TestScheduler { inner, spawner, syscalls }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (wrapped, handle) = crate::task::JoinHandle::<F::Output>::wrap(future);
        let future = self.syscalls.wrap_future(wrapped);
        self.inner.spawn(future);
        return handle;
    }

    pub(crate) fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.block_on(future)
    }
}

impl fmt::Debug for TestScheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("TestScheduler").finish()
    }
}

impl Spawner {
    /// Spawns a future onto the scheduler.
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (wrapped, handle) = crate::task::JoinHandle::<F::Output>::wrap(future);
        let future = self.syscalls.wrap_future(wrapped);
        self.inner.spawn(future);
        return handle;
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}
