//! Implementer notes:
//! Right now, the plan is to mirror `basic_scheduler`. We can then intercept
//! calls to spawn and inject a task ID. Further, we're able to drive the runtime
//! and the `Syscalls` trait at the same time.
use crate::park::Park;
use crate::runtime::BasicScheduler;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
mod park;
use crate::syscall::Syscalls;
use crate::task::JoinHandle;

/// Deterministically executes tasks on the current thread,
pub(crate) struct TestScheduler<P>
where
    P: Park,
{
    inner: BasicScheduler<park::SyscallsPark<P>>,
    spawner: Spawner,
}

#[derive(Clone)]
pub(crate) struct Spawner {
    inner: crate::runtime::basic_scheduler::Spawner,
}

impl<P> TestScheduler<P>
where
    P: Park,
{
    fn new(park: P, syscalls: Arc<dyn Syscalls>) -> Self {
        let park = park::SyscallsPark::new(park, syscalls);
        let inner = BasicScheduler::new(park);
        let spawner = Spawner {
            inner: inner.spawner().clone(),
        };
        TestScheduler { inner, spawner }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(future)
    }

    pub(crate) fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.block_on(future)
    }
}

impl<P: Park> fmt::Debug for TestScheduler<P> {
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
        todo!()
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}
