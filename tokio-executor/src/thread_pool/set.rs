//! Putting a worker to sleep.
//!
//! - Attempt to spin.

use crate::loom::rand::seed;
use crate::loom::sync::Arc;
use crate::park::Unpark;
use crate::task::{self, Task};
use crate::thread_pool::{current, queue, BoxFuture, Idle, JoinHandle, Owned, Shared};
use crate::util::{CachePadded, FastRand};
use crate::{Executor, SpawnError};

use std::cell::UnsafeCell;
use std::future::Future;

pub(crate) struct Set<P>
where
    P: 'static,
{
    /// Data accessible from all workers.
    shared: Box<[Shared<P>]>,

    /// Data owned by the worker.
    owned: Box<[UnsafeCell<CachePadded<Owned<P>>>]>,

    /// Submit work to the pool while *not* currently on a worker thread.
    inject: queue::Inject<Shared<P>>,

    /// Coordinates idle workers
    idle: Idle,

    /// Pool where blocking tasks should be spawned.
    pub(crate) blocking: Arc<crate::blocking::Pool>,
}

unsafe impl<P: Unpark> Send for Set<P> {}
unsafe impl<P: Unpark> Sync for Set<P> {}

impl<P> Set<P>
where
    P: Unpark,
{
    /// Create a new worker set using the provided queues.
    pub(crate) fn new<F>(
        num_workers: usize,
        mut mk_unpark: F,
        blocking: Arc<crate::blocking::Pool>,
    ) -> Self
    where
        F: FnMut(usize) -> P,
    {
        assert!(num_workers > 0);

        let queues = queue::build(num_workers);
        let inject = queues[0].injector();

        let mut shared = Vec::with_capacity(queues.len());
        let mut owned = Vec::with_capacity(queues.len());

        for (i, queue) in queues.into_iter().enumerate() {
            let unpark = mk_unpark(i);
            let rand = FastRand::new(seed());

            shared.push(Shared::new(unpark));
            owned.push(UnsafeCell::new(CachePadded::new(Owned::new(queue, rand))));
        }

        Set {
            shared: shared.into_boxed_slice(),
            owned: owned.into_boxed_slice(),
            inject,
            idle: Idle::new(num_workers),
            blocking,
        }
    }

    fn inject_task(&self, task: Task<Shared<P>>) {
        self.inject.push(task, |res| {
            if let Err(task) = res {
                task.shutdown();

                // There may be a worker, in the process of being shutdown, that is
                // waiting for this task to be released, so we notify all workers
                // just in case.
                //
                // Over aggressive, but the runtime is in the process of shutting
                // down, so efficiency is not critical.
                self.notify_all();
            } else {
                self.notify_work();
            }
        });
    }

    pub(super) fn notify_work(&self) {
        if let Some(index) = self.idle.worker_to_notify() {
            self.shared[index].unpark();
        }
    }

    pub(super) fn notify_all(&self) {
        for shared in &self.shared[..] {
            shared.unpark();
        }
    }

    pub(crate) fn spawn_background<F>(&self, future: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let task = task::background(future);
        self.schedule(task);
    }

    pub(super) fn blocking_pool(&self) -> &Arc<crate::blocking::Pool> {
        &self.blocking
    }

    pub(crate) fn schedule(&self, task: Task<Shared<P>>) {
        current::get(|current_worker| match current_worker.as_member(self) {
            Some(worker) => {
                if worker.submit_local(task) {
                    self.notify_work();
                }
            }
            None => {
                self.inject_task(task);
            }
        })
    }

    pub(crate) fn set_container_ptr(&mut self) {
        let ptr = self as *const _;
        for shared in &mut self.shared[..] {
            shared.set_container_ptr(ptr);
        }
    }

    /// Signal the pool is closed
    ///
    /// Returns `true` if the transition to closed is successful. `false`
    /// indicates the pool was already closed.
    pub(crate) fn close(&self) -> bool {
        if self.inject.close() {
            self.notify_all();
            true
        } else {
            false
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.shared.len()
    }

    pub(super) fn index_of(&self, shared: &Shared<P>) -> usize {
        use std::mem;

        let size = mem::size_of::<Shared<P>>();

        ((shared as *const _ as usize) - (&self.shared[0] as *const _ as usize)) / size
    }

    pub(super) fn shared(&self) -> &[Shared<P>] {
        &self.shared
    }

    pub(super) fn owned(&self) -> &[UnsafeCell<CachePadded<Owned<P>>>] {
        &self.owned
    }

    pub(super) fn idle(&self) -> &Idle {
        &self.idle
    }
}

impl<P: 'static> Drop for Set<P> {
    fn drop(&mut self) {
        // Before proceeding, wait for all concurrent wakers to exit
        self.inject.wait_for_unlocked();
    }
}

impl Set<Box<dyn Unpark>> {
    pub(crate) fn spawn_typed<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.schedule(task);
        JoinHandle::new(handle)
    }
}

impl<P> Executor for &Set<P>
where
    P: Unpark,
{
    fn spawn(&mut self, future: BoxFuture) -> Result<(), SpawnError> {
        self.spawn_background(future);
        Ok(())
    }

    fn status(&self) -> Result<(), SpawnError> {
        Ok(())
    }
}
