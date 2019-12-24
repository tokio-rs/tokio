//! The scheduler is divided into multiple slices. Each slice is fairly
//! isolated, having its own queue. A worker is dedicated to processing a single
//! slice.

use crate::loom::rand::seed;
use crate::park::Park;
use crate::runtime::thread_pool::{current, queue, Idle, Owned, Shared};
use crate::runtime::Parker;
use crate::task::{self, JoinHandle, Task};
use crate::util::{CachePadded, FastRand};

use std::cell::UnsafeCell;
use std::future::Future;

pub(super) struct Set {
    /// Data accessible from all workers.
    shared: Box<[Shared]>,

    /// Data owned by the worker.
    owned: Box<[UnsafeCell<CachePadded<Owned>>]>,

    /// Submit work to the pool while *not* currently on a worker thread.
    inject: queue::Inject<Shared>,

    /// Coordinates idle workers
    idle: Idle,
}

unsafe impl Send for Set {}
unsafe impl Sync for Set {}

impl Set {
    /// Creates a new worker set using the provided queues.
    pub(crate) fn new(parkers: &[Parker]) -> Self {
        assert!(!parkers.is_empty());

        let queues = queue::build(parkers.len());
        let inject = queues[0].injector();

        let mut shared = Vec::with_capacity(queues.len());
        let mut owned = Vec::with_capacity(queues.len());

        for (i, queue) in queues.into_iter().enumerate() {
            let rand = FastRand::new(seed());

            shared.push(Shared::new(parkers[i].unpark()));
            owned.push(UnsafeCell::new(CachePadded::new(Owned::new(queue, rand))));
        }

        Set {
            shared: shared.into_boxed_slice(),
            owned: owned.into_boxed_slice(),
            inject,
            idle: Idle::new(parkers.len()),
        }
    }

    pub(crate) fn spawn_typed<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.schedule(task);
        handle
    }

    fn inject_task(&self, task: Task<Shared>) {
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

    pub(crate) fn schedule(&self, task: Task<Shared>) {
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

    pub(crate) fn set_ptr(&mut self) {
        let ptr = self as *const _;
        for shared in &mut self.shared[..] {
            shared.set_slices_ptr(ptr);
        }
    }

    /// Signals the pool is closed
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

    pub(crate) fn is_closed(&self) -> bool {
        self.inject.is_closed()
    }

    pub(crate) fn len(&self) -> usize {
        self.shared.len()
    }

    pub(super) fn index_of(&self, shared: &Shared) -> usize {
        use std::mem;

        let size = mem::size_of::<Shared>();

        ((shared as *const _ as usize) - (&self.shared[0] as *const _ as usize)) / size
    }

    pub(super) fn shared(&self) -> &[Shared] {
        &self.shared
    }

    pub(super) fn owned(&self) -> &[UnsafeCell<CachePadded<Owned>>] {
        &self.owned
    }

    pub(super) fn idle(&self) -> &Idle {
        &self.idle
    }

    /// Waits for all locks on the injection queue to drop.
    ///
    /// This is done by locking w/o doing anything.
    pub(super) fn wait_for_unlocked(&self) {
        self.inject.wait_for_unlocked();
    }
}

impl Drop for Set {
    fn drop(&mut self) {
        // Before proceeding, wait for all concurrent wakers to exit
        self.wait_for_unlocked();
    }
}
