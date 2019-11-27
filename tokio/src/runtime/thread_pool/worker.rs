use crate::loom::cell::CausalCell;
use crate::loom::sync::Arc;
use crate::park::Park;
use crate::runtime::{self, blocking};
use crate::runtime::park::Parker;
use crate::runtime::thread_pool::{current, slice, Owned, Shared, Spawner};
use crate::task::Task;

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

thread_local! {
    /// Used to handle block_in_place
    static ON_BLOCK: Cell<Option<*const dyn Fn()>> = Cell::new(None)
}

cfg_blocking! {
    pub(crate) fn block_in_place<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // Make the current worker give away its Worker to another thread so that we can safely block
        // this one without preventing progress on other futures the worker owns.
        ON_BLOCK.with(|ob| {
            let allow_blocking = ob
                .get()
                .expect("can only call blocking when on Tokio runtime");

            // This is safe, because ON_BLOCK was set from an &mut dyn FnMut in the worker that wraps
            // the worker's operation, and is unset just prior to when the FnMut is dropped.
            let allow_blocking = unsafe { &*allow_blocking };

            allow_blocking();
            f()
        })
    }
}

pub(crate) struct Worker {
    /// Parks the thread. Requires the calling worker to have obtained unique
    /// access via the generation synchronization action.
    inner: Arc<Inner>,

    /// Scheduler slices
    slices: Arc<slice::Set>,

    /// Slice assigned to this worker
    index: usize,

    /// Worker generation. This is used to synchronize access to the internal
    /// data.
    generation: usize,

    /// To indicate that the Worker has been given away and should no longer be used
    gone: Cell<bool>,
}

/// Internal worker state. This may be referenced from multiple threads, but the
/// generation guard protects unsafe access
struct Inner {
    /// Used to park the thread
    park: CausalCell<Parker>,
}

unsafe impl Send for Worker {}

/// Used to ensure the invariants are respected
struct GenerationGuard<'a> {
    /// Worker reference
    worker: &'a Worker,

    /// Prevent `Sync` access
    _p: PhantomData<Cell<()>>,
}

struct WorkerGone;

// TODO: Move into slices
pub(super) fn create_set(
    pool_size: usize,
    parker: Parker,
) -> (Arc<slice::Set>, Vec<Worker>) {
    // Create the parks...
    let parkers: Vec<_> = (0..pool_size).map(|_| parker.clone()).collect();

    let mut slices = Arc::new(slice::Set::new(&parkers));

    // Establish the circular link between the individual worker state
    // structure and the container.
    Arc::get_mut(&mut slices).unwrap().set_ptr();

    // This will contain each worker.
    let workers = parkers
        .into_iter()
        .enumerate()
        .map(|(index, parker)| {
            Worker::new(
                slices.clone(),
                index,
                parker,
            )
        })
        .collect();

    (slices, workers)
}

/// After how many ticks is the global queue polled. This helps to ensure
/// fairness.
///
/// The number is fairly arbitrary. I believe this value was copied from golang.
const GLOBAL_POLL_INTERVAL: u16 = 61;

impl Worker {
    // Safe as aquiring a lock is required before doing anything potentially
    // dangerous.
    pub(super) fn new(
        slices: Arc<slice::Set>,
        index: usize,
        park: Parker,
    ) -> Self {
        Worker {
            inner: Arc::new(Inner {
                park: CausalCell::new(park),
            }),
            slices,
            index,
            generation: 0,
            gone: Cell::new(false),
        }
    }

    pub(super) fn run(self, blocking_pool: blocking::Spawner) {
        // First, acquire a lock on the worker.
        let guard = match self.acquire_lock() {
            Some(guard) => guard,
            None => return,
        };

        let spawner = Spawner::new(self.slices.clone());

        // Track the current worker
        current::set(&self.slices, self.index, || {
            // Enter a runtime context
            let _enter = crate::runtime::enter();

            crate::runtime::global::with_thread_pool(&spawner, || {
                blocking_pool.enter(|| {
                    ON_BLOCK.with(|ob| {
                        // Ensure that the ON_BLOCK is removed from the thread-local context
                        // when leaving the scope. This handles cases that involve panicking.
                        struct Reset<'a>(&'a Cell<Option<*const dyn Fn()>>);

                        impl<'a> Drop for Reset<'a> {
                            fn drop(&mut self) {
                                self.0.set(None);
                            }
                        }

                        let _reset = Reset(ob);

                        let allow_blocking: &dyn Fn() = &|| self.block_in_place(&blocking_pool);

                        ob.set(Some(unsafe {
                            // NOTE: We cannot use a safe cast to raw pointer here, since we are
                            // _also_ erasing the lifetime of these pointers. That is safe here,
                            // because we know that ob will set back to None before allow_blocking
                            // is dropped.
                            #[allow(clippy::useless_transmute)]
                            std::mem::transmute::<_, *const dyn Fn()>(allow_blocking)
                        }));

                        let _ = guard.run();

                        // Ensure that we reset ob before allow_blocking is dropped.
                        drop(_reset);
                    });
                })
            })
        });

        if self.gone.get() {
            // Synchronize with the pool for load(Acquire) in is_closed to get
            // up-to-date value.
            self.slices.wait_for_unlocked();

            if self.slices.is_closed() {
                // If the pool is shutting down, some other thread may be
                // waiting to clean up after the task that we were holding on
                // to. If we completed that task, we did nothing (because
                // task.run() returned None), and so crucially we did not wait
                // up any such thread.
                //
                // So, we have to do that here.
                self.slices.notify_all();
            }
        }
    }

    /// Acquire the lock
    fn acquire_lock(&self) -> Option<GenerationGuard<'_>> {
        // Safety: Only getting `&self` access to access atomic field
        let owned = unsafe { &*self.slices.owned()[self.index].get() };

        // The lock is only to establish mutual exclusion. Other synchronization
        // handles memory orderings
        let prev = owned.generation.compare_and_swap(
            self.generation,
            self.generation.wrapping_add(1),
            Relaxed,
        );

        if prev == self.generation {
            Some(GenerationGuard {
                worker: self,
                _p: PhantomData,
            })
        } else {
            None
        }
    }

    /// Enter an in-place blocking section
    fn block_in_place(&self, blocking_pool: &blocking::Spawner) {
        // If our Worker has already been given away, then blocking is fine!
        if self.gone.get() {
            return;
        }

        // make sure no subsequent code thinks that it is on a worker
        current::clear();

        // Track that the worker is gone
        self.gone.set(true);

        // If this method is called, we need to move the entire worker onto a
        // separate (blocking) thread before returning. Once we return, the
        // caller is going to execute some blocking code which would otherwise
        // block our reactor from making progress. Since we are _in the middle_
        // of running a task, this isn't trivial, as the Worker is "active".
        // We do have the luxury of knowing that we are on the worker thread,
        // so we can assert exclusive access to any Worker-specific state.
        //
        // More specifically, the caller is _currently_ "stuck" in
        // Entry::run_task at:
        //
        //   if let Some(task) = task.run(self.shared().into()) {
        //
        // And _we_ get to decide when it continues (specifically, by choosing
        // when we return from the second callback (i.e., after the FnOnce
        // passed to blocking has returned).
        //
        // Here's what we'll have to do:
        //
        //  - Reconstruct our `Worker` struct
        //  - Spawn the reconstructed `Worker` on another blocking thread
        //  - Clear any state indicating what worker we are on, since at this
        //    point we are effectively no longer "on" that worker.
        //  - Allow the caller of `blocking` to continue.
        //
        // Once the caller completes the blocking operations, we need to ensure
        // that async code can continue running in that context. Luckily, since
        // `Arc<slice::Set>` has a fallback for when
        // current::get() is None, we can just let the task
        // run until it yields, and then put it back into
        // the pool.

        let worker = Worker {
            inner: self.inner.clone(),
            slices: self.slices.clone(),
            index: self.index,
            generation: self.generation + 1,
            gone: Cell::new(false),
        };

        // Give away the worker
        let b = blocking_pool.clone();
        runtime::spawn_blocking(move || worker.run(b));
    }
}

impl GenerationGuard<'_> {
    fn run(self) -> Result<(), WorkerGone> {
        let mut me = self;

        while me.is_running() {
            me = me.process_available_work()?;

            if me.is_running() {
                me.park();
            }
        }

        me.shutdown();
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.owned().is_running.get()
    }

    /// Returns `true` if the worker needs to park
    fn process_available_work(self) -> Result<Self, WorkerGone> {
        let mut me = self;

        loop {
            // Local queue loop
            loop {
                let task = match me.find_local_work() {
                    Some(task) => task,
                    None => {
                        if !me.is_running() {
                            // The scheduler is in the process of shutting down.
                            return Ok(me);
                        }

                        // Break out of the local task loop and try to steal
                        break;
                    }
                };

                me = me.run_task(task)?;
            }

            // No more **local** work to process, try transitioning to searching
            // in order to attempt to steal work from other workers.
            //
            // On `false`, the worker has entered the parked state
            if me.transition_to_searching() {
                // Try to steal tasks from other workers
                if let Some(task) = me.steal_work() {
                    me = me.run_task(task)?;
                } else {
                    // No work to steal, perform some routine work
                    me.drain_tasks_pending_drop();
                    return Ok(me);
                }
            } else {
                return Ok(me);
            }

            // Start checking the local queue again
        }
    }

    /// Find local work
    fn find_local_work(&mut self) -> Option<Task<Shared>> {
        let tick = self.tick_fetch_inc();

        if tick % GLOBAL_POLL_INTERVAL == 0 {
            // Sleep light...
            self.park_light();

            // Perform regularly scheduled maintenance work.
            self.maintenance();

            if !self.is_running() {
                return None;
            }

            // Check the global queue
            self.owned().work_queue.pop_global_first()
        } else {
            self.owned().work_queue.pop_local_first()
        }
    }

    fn steal_work(&mut self) -> Option<Task<Shared>> {
        let num_slices = self.worker.slices.len();
        let start = self.owned().rand.fastrand_n(num_slices as u32);

        self.owned()
            .work_queue
            .steal(start as usize)
            // Fallback on checking the local queue, which will also check the
            // injector.
            .or_else(|| self.owned().work_queue.pop_global_first())
    }

    /// Runs maintenance work such as free pending tasks and check the pool's
    /// state.
    fn maintenance(&mut self) {
        // Free any completed tasks
        self.drain_tasks_pending_drop();

        // Update the pool state cache
        let closed = self.owned().work_queue.is_closed();
        self.owned().is_running.set(!closed)
    }

    fn transition_to_searching(&mut self) -> bool {
        if self.is_searching() {
            return true;
        }

        let ret = self.slices().idle().transition_worker_to_searching();
        self.owned().is_searching.set(ret);
        ret
    }

    fn transition_from_searching(&mut self) {
        self.owned().is_searching.set(false);

        if self.slices().idle().transition_worker_from_searching() {
            // We are the final searching worker. Because work was found, we
            // need to notify another worker.
            self.slices().notify_work();
        }
    }

    /// Returns `true` if the worker must check for any work.
    fn transition_to_parked(&mut self) -> bool {
        let idx = self.index();
        let is_searching = self.is_searching();
        let ret = self
            .slices()
            .idle()
            .transition_worker_to_parked(idx, is_searching);

        // The worker is no longer searching. Setting this is the local cache
        // only.
        self.owned().is_searching.set(false);

        // When tasks are submitted locally (from the parker), defer any
        // notifications in hopes that the curent worker will grab those tasks.
        self.owned().defer_notification.set(true);

        ret
    }

    /// Returns `true` if the transition happened.
    fn transition_from_parked(&mut self) -> bool {
        if self.owned().did_submit_task.get() || !self.is_running() {
            // Remove the worker from the sleep set.
            self.slices().idle().unpark_worker_by_id(self.index());

            self.owned().is_searching.set(true);
            self.owned().defer_notification.set(false);

            true
        } else {
            let ret = !self.slices().idle().is_parked(self.index());

            if ret {
                self.owned().is_searching.set(true);
                self.owned().defer_notification.set(false);
            }

            ret
        }
    }

    /// Runs the task. During the task execution, it is possible for worker to
    /// transition to a new thread. In this case, the caller loses the guard to
    /// access the generation and must stop processing.
    fn run_task(mut self, task: Task<Shared>) -> Result<Self, WorkerGone> {
        if self.is_searching() {
            self.transition_from_searching();
        }

        let gone = &self.worker.gone;
        let executor = self.shared();

        let task = task.run(&mut || {
            if gone.get() {
                None
            } else {
                Some(executor.into())
            }
        });

        if gone.get() {
            // The Worker disappeared from under us.
            // We need to return, because we no longer own all of our state!
            // Make sure the task gets picked up again eventually.
            if let Some(task) = task {
                self.worker.slices.schedule(task);
            }

            Err(WorkerGone)
        } else {
            if let Some(task) = task {
                self.owned().submit_local_yield(task);
                self.slices().notify_work();
            }

            Ok(self)
        }
    }

    fn final_work_sweep(&mut self) {
        if !self.owned().work_queue.is_empty() {
            self.slices().notify_work();
        }
    }

    fn park(&mut self) {
        if self.transition_to_parked() {
            // We are the final searching worker, check if any work arrived
            // before parking
            self.final_work_sweep();
        }

        // The state has been transitioned to parked, we can now wait by
        // calling the parker. This is done in a loop as spurious wakeups are
        // permitted.
        loop {
            self.park_mut().park().expect("park failed");

            // We might have been woken to clean up a dropped task
            self.maintenance();

            if self.transition_from_parked() {
                return;
            }
        }
    }

    fn park_light(&mut self) {
        // When tasks are submitted locally (from the parker), defer any
        // notifications in hopes that the curent worker will grab those tasks.
        self.owned().defer_notification.set(true);

        self.park_mut()
            .park_timeout(Duration::from_millis(0))
            .expect("park failed");

        self.owned().defer_notification.set(false);

        if self.owned().did_submit_task.get() {
            self.slices().notify_work();
            self.owned().did_submit_task.set(false)
        }
    }

    fn drain_tasks_pending_drop(&mut self) {
        for task in self.shared().pending_drop.drain() {
            unsafe {
                let owned = &mut *self.slices().owned()[self.index()].get();
                owned.release_task(&task);
            }
            drop(task);
        }
    }

    /// Shutdown the worker.
    ///
    /// Once the shutdown flag has been observed, it is guaranteed that no
    /// further tasks may be pushed into the global queue.
    fn shutdown(&mut self) {
        // Transition all tasks owned by the worker to canceled.
        self.owned().owned_tasks.shutdown();

        // First, drain all tasks from both the local & global queue.
        while let Some(task) = self.owned().work_queue.pop_local_first() {
            task.shutdown();
        }

        // Notify all workers in case they have pending tasks to drop
        //
        // Not super efficient, but we are also shutting down.
        self.worker.slices.notify_all();

        // The worker can only shutdown once there are no further owned tasks.
        while !self.owned().owned_tasks.is_empty() {
            // Wait until task that this worker owns are released.
            //
            // `transition_to_parked` is not called as we are not working
            // anymore. When a task is released, the owning worker is unparked
            // directly.
            self.park_mut().park().expect("park failed");

            // Try draining more tasks
            self.drain_tasks_pending_drop();
        }
    }

    /// Increment the tick, returning the value from before the increment.
    fn tick_fetch_inc(&mut self) -> u16 {
        let tick = self.owned().tick.get();
        self.owned().tick.set(tick.wrapping_add(1));
        tick
    }

    fn is_searching(&self) -> bool {
        self.owned().is_searching.get()
    }

    fn index(&self) -> usize {
        self.worker.index
    }

    fn slices(&self) -> &slice::Set {
        &self.worker.slices
    }

    fn shared(&self) -> &Shared {
        &self.slices().shared()[self.index()]
    }

    fn owned(&self) -> &Owned {
        let index = self.index();
        // safety: we own the slot
        unsafe { &*self.slices().owned()[index].get() }
    }

    fn park_mut(&mut self) -> &mut Parker {
        // Safety: `&mut self` on `GenerationGuard` implies it is safe to
        // perform the action.
        unsafe { self.worker.inner.park.with_mut(|ptr| &mut *ptr) }
    }
}
