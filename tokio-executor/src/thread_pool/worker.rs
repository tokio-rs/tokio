use crate::loom::sync::Arc;
use crate::park::{Park, Unpark};
use crate::task::Task;
use crate::thread_pool::{current, Owned, Shared};

use std::time::Duration;

// TODO: remove this re-export
pub(super) use crate::thread_pool::set::Set;

pub(crate) struct Worker<P: Park + 'static> {
    /// Entry in the set of workers.
    entry: Entry<P::Unpark>,

    /// Park the thread
    park: P,
}

pub(crate) fn create_set<F, P>(
    pool_size: usize,
    mk_park: F,
    blocking: Arc<crate::blocking::Pool>,
) -> (Arc<Set<P::Unpark>>, Vec<Worker<P>>)
where
    P: Send + Park,
    F: FnMut(usize) -> P,
{
    // Create the parks...
    let parks: Vec<_> = (0..pool_size).map(mk_park).collect();

    let mut pool = Arc::new(Set::new(pool_size, |i| parks[i].unpark(), blocking));

    // Establish the circular link between the individual worker state
    // structure and the container.
    Arc::get_mut(&mut pool).unwrap().set_container_ptr();

    // This will contain each worker.
    let workers = parks
        .into_iter()
        .enumerate()
        .map(|(index, park)| {
            // unsafe is safe because we call Worker::new only once with each index in the pool
            unsafe { Worker::new(pool.clone(), index, park) }
        })
        .collect();

    (pool, workers)
}

/// After how many ticks is the global queue polled. This helps to ensure
/// fairness.
///
/// The number is fairly arbitrary. I believe this value was copied from golang.
const GLOBAL_POLL_INTERVAL: u16 = 61;

impl<P> Worker<P>
where
    P: Send + Park,
{
    // unsafe because new may only be called once for each index in pool's set
    pub(super) unsafe fn new(pool: Arc<Set<P::Unpark>>, index: usize, park: P) -> Self {
        Worker {
            entry: Entry::new(pool, index),
            park,
        }
    }

    pub(super) fn run(mut self) {
        let pool = Arc::clone(&self.entry.pool);
        let pool = &pool;
        let index = self.entry.index;

        let mut executor = &**pool;
        let entry = &mut self.entry;
        let park = &mut self.park;

        let blocking = &executor.blocking;

        // Track the current worker
        current::set(&pool, index, || {
            let _enter = crate::enter().expect("executor already running on thread");

            crate::with_default(&mut executor, || {
                crate::blocking::with_pool(blocking, || entry.run(park))
            })
        });
    }

    pub(super) fn id(&self) -> usize {
        self.entry.index
    }

    #[cfg(test)]
    #[allow(warnings)]
    pub(crate) fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        current::set(&self.entry.pool, self.entry.index, f)
    }

    #[cfg(test)]
    #[allow(warnings)]
    pub(crate) fn tick(&mut self) {
        self.entry.tick(&mut self.park);
    }
}

struct Entry<P: 'static> {
    pool: Arc<Set<P>>,
    index: usize,
}

impl<P> Entry<P>
where
    P: Unpark,
{
    // unsafe because Entry::owned assumes there is only one instance of the Entry
    unsafe fn new(pool: Arc<Set<P>>, index: usize) -> Self {
        Entry { pool, index }
    }

    fn run(&mut self, park: &mut impl Park<Unpark = P>) {
        while self.is_running() {
            if self.tick(park) {
                self.park(park);
            }
        }

        self.shutdown(park);
    }

    fn is_running(&mut self) -> bool {
        self.owned().is_running.get()
    }

    /// Returns `true` if the worker needs to park
    fn tick(&mut self, park: &mut impl Park<Unpark = P>) -> bool {
        // Process all pending tasks in the local queue.
        if !self.process_local_queue(park) {
            return false;
        }

        // No more **local** work to process, try transitioning to searching
        // in order to attempt to steal work from other workers.
        //
        // On `false`, the worker has entered the parked state
        if self.transition_to_searching() {
            // If `true` then work was found
            if self.search_for_work() {
                return false;
            }
        }

        true
    }

    /// Process all pending tasks in the local queue, occasionally checking the
    /// global queue, but never other worker local queues.
    ///
    /// Returns `false` if processing was interrupted due to the pool shutting
    /// down.
    fn process_local_queue(&mut self, park: &mut impl Park<Unpark = P>) -> bool {
        debug_assert!(self.is_running());

        loop {
            let tick = self.tick_fetch_inc();

            let task = if tick % GLOBAL_POLL_INTERVAL == 0 {
                // Sleep light...
                self.park_light(park);

                // Perform regularly scheduled maintenance work.
                self.maintenance();

                if !self.is_running() {
                    return false;
                }

                // Check the global queue
                self.owned().work_queue.pop_global_first()
            } else {
                self.owned().work_queue.pop_local_first()
            };

            if let Some(task) = task {
                self.run_task(task);
            } else {
                return true;
            }
        }
    }

    fn steal_work(&mut self) -> Option<Task<Shared<P>>> {
        let num_workers = self.pool.len();
        let start = self.owned().rand.fastrand_n(num_workers as u32);

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

    fn search_for_work(&mut self) -> bool {
        debug_assert!(self.is_searching());

        if let Some(task) = self.steal_work() {
            self.run_task(task);
            true
        } else {
            // Perform some routine work
            self.drain_tasks_pending_drop();
            false
        }
    }

    fn transition_to_searching(&mut self) -> bool {
        if self.is_searching() {
            return true;
        }

        let ret = self.set().idle().transition_worker_to_searching();
        self.owned().is_searching.set(ret);
        ret
    }

    fn transition_from_searching(&mut self) {
        debug_assert!(self.is_searching());

        self.owned().is_searching.set(false);

        if self.set().idle().transition_worker_from_searching() {
            // We are the final searching worker. Because work was found, we
            // need to notify another worker.
            self.set().notify_work();
        }
    }

    /// Returns `true` if the worker must check for any work.
    fn transition_to_parked(&mut self) -> bool {
        let idx = self.index;
        let is_searching = self.is_searching();
        let ret = self
            .set()
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
            self.set().idle().unpark_worker_by_id(self.index);

            self.owned().is_searching.set(true);
            self.owned().defer_notification.set(false);

            true
        } else {
            let ret = !self.set().idle().is_parked(self.index);

            if ret {
                self.owned().is_searching.set(true);
                self.owned().defer_notification.set(false);
            }

            ret
        }
    }

    fn run_task(&mut self, task: Task<Shared<P>>) {
        if self.is_searching() {
            self.transition_from_searching();
        }

        if let Some(task) = task.run(self.shared().into()) {
            self.owned().submit_local_yield(task);
            self.set().notify_work();
        }
    }

    fn final_work_sweep(&mut self) {
        if !self.owned().work_queue.is_empty() {
            self.set().notify_work();
        }
    }

    fn park(&mut self, park: &mut impl Park<Unpark = P>) {
        if self.transition_to_parked() {
            // We are the final searching worker, check if any work arrived
            // before parking
            self.final_work_sweep();
        }

        // The state has been transitioned to parked, we can now wait by
        // calling the parker. This is done in a loop as spurious wakeups are
        // permitted.
        loop {
            park.park().ok().expect("park failed");

            // We might have been woken to clean up a dropped task
            self.maintenance();

            if self.transition_from_parked() {
                return;
            }
        }
    }

    fn park_light(&mut self, park: &mut impl Park<Unpark = P>) {
        // When tasks are submitted locally (from the parker), defer any
        // notifications in hopes that the curent worker will grab those tasks.
        self.owned().defer_notification.set(true);

        park.park_timeout(Duration::from_millis(0))
            .ok()
            .expect("park failed");

        self.owned().defer_notification.set(false);

        if self.owned().did_submit_task.get() {
            self.set().notify_work();
            self.owned().did_submit_task.set(false)
        }
    }

    fn drain_tasks_pending_drop(&mut self) {
        for task in self.shared().pending_drop.drain() {
            unsafe {
                let owned = &mut *self.set().owned()[self.index].get();
                owned.release_task(&task);
            }
            drop(task);
        }
    }

    /// Shutdown the worker.
    ///
    /// Once the shutdown flag has been observed, it is guaranteed that no
    /// further tasks may be pushed into the global queue.
    fn shutdown(&mut self, park: &mut impl Park<Unpark = P>) {
        // Transition all tasks owned by the worker to canceled.
        self.owned().owned_tasks.shutdown();

        // First, drain all tasks from both the local & global queue.
        while let Some(task) = self.owned().work_queue.pop_local_first() {
            task.shutdown();
        }

        // Notify all workers in case they have pending tasks to drop
        //
        // Not super efficient, but we are also shutting down.
        self.pool.notify_all();

        // The worker can only shutdown once there are no further owned tasks.
        while !self.owned().owned_tasks.is_empty() {
            // Wait until task that this worker owns are released.
            //
            // `transition_to_parked` is not called as we are not working
            // anymore. When a task is released, the owning worker is unparked
            // directly.
            park.park().ok().expect("park failed");

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

    fn is_searching(&mut self) -> bool {
        self.owned().is_searching.get()
    }

    fn set(&self) -> &Set<P> {
        &self.pool
    }

    fn shared(&self) -> &Shared<P> {
        &self.set().shared()[self.index]
    }

    fn owned(&mut self) -> &Owned<P> {
        // safety: we own the slot
        unsafe { &*self.set().owned()[self.index].get() }
    }
}
