use crate::executor::loom::sync::Arc;
use crate::executor::park::{Park, Unpark};
use crate::executor::task::Task;
use crate::executor::thread_pool::{current, Owned, Shared, Spawner};

use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::time::Duration;

// The Arc<Box<_>> is needed because loom doesn't support Arc<T> where T: !Sized
// loom doesn't support that because it requires CoerceUnsized, which is unstable
type LaunchWorker<P> = Arc<Box<dyn Fn(Worker<P>) -> Box<dyn FnOnce() + Send> + Send + Sync>>;

thread_local! {
    /// Thread-local tracking the current executor
    static ON_BLOCK: Cell<Option<*mut dyn FnMut()>> = Cell::new(None)
}

/// Run the provided blocking function without blocking the executor.
///
/// In general, issuing a blocking call or performing a lot of compute in a future without
/// yielding is not okay, as it may prevent the executor from driving other futures forward.
/// If you run a closure through this method, the current executor thread will relegate all its
/// executor duties to another (possibly new) thread, and only then poll the task. Note that this
/// requires additional synchronization.
///
/// # Examples
///
/// ```
/// # async fn docs() {
/// tokio::executor::thread_pool::blocking(move || {
///     // do some compute-heavy work or call synchronous code
/// });
/// # }
/// ```
#[cfg(feature = "blocking")]
pub fn blocking<F, R>(f: F) -> R
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
        let allow_blocking = unsafe { &mut *allow_blocking };

        allow_blocking();
        f()
    })
}

// TODO: remove this re-export
pub(super) use crate::executor::thread_pool::set::Set;

pub(crate) struct Worker<P: Park + 'static> {
    /// Entry in the set of workers.
    entry: Entry<P::Unpark>,

    /// Park the thread
    park: Box<P>,

    /// Fn for launching another Worker should we need it
    launch_worker: LaunchWorker<P>,

    /// To indicate that the Worker has been given away and should no longer be used
    gone: Cell<bool>,
}

pub(super) fn create_set<F, P>(
    pool_size: usize,
    mk_park: F,
    launch_worker: LaunchWorker<P>,
    blocking: Arc<crate::executor::blocking::Pool>,
) -> (Arc<Set<P::Unpark>>, Vec<Worker<P>>)
where
    P: Send + Park,
    F: FnMut(usize) -> Box<P>,
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
            unsafe { Worker::new(pool.clone(), index, park, Arc::clone(&launch_worker)) }
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
    pub(super) unsafe fn new(
        pool: Arc<Set<P::Unpark>>,
        index: usize,
        park: Box<P>,
        launch_worker: LaunchWorker<P>,
    ) -> Self {
        Worker {
            entry: Entry::new(pool, index),
            park,
            launch_worker,
            gone: Cell::new(false),
        }
    }

    pub(super) fn run(mut self)
    where
        P: Park<Unpark = Box<dyn Unpark>>,
    {
        let pool = Arc::clone(&self.entry.pool);
        let pool = &pool;
        let index = self.entry.index;

        let executor = &**pool;
        let spawner = Spawner::new(pool.clone());
        let entry = &mut self.entry;
        let launch_worker = &self.launch_worker;

        let blocking = &executor.blocking;
        let gone = &self.gone;

        let mut park = DropNotGone::new(self.park, gone);

        // Track the current worker
        current::set(&pool, index, || {
            let _enter = crate::executor::enter().expect("executor already running on thread");

            crate::executor::global::with_thread_pool(&spawner, || {
                crate::executor::blocking::with_pool(blocking, || {
                    ON_BLOCK.with(|ob| {
                        // Ensure that the ON_BLOCK is removed from the thread-local context
                        // when leaving the scope. This handles cases that involve panicking.
                        struct Reset<'a>(&'a Cell<Option<*mut dyn FnMut()>>);

                        impl<'a> Drop for Reset<'a> {
                            fn drop(&mut self) {
                                self.0.set(None);
                            }
                        }

                        let _reset = Reset(ob);

                        let park_ptr = &mut **park as *mut _;
                        let mut allow_blocking = move || {
                            // If our Worker has already been given away, then blocking is fine!
                            if gone.get() {
                                return;
                            }

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
                            //    - Notably, this includes `park`, which we're passing in below.
                            //  - Spawn the reconstructed `Worker` on another blocking thread
                            //  - Clear any state indicating what worker we are on, since at this
                            //    point we are effectively no longer "on" that worker.
                            //  - Allow the caller of `blocking` to continue.
                            //
                            // TODO: should we also undo the enter()?
                            //
                            // Once the caller completes the blocking operations, we need to ensure
                            // that async code can continue running in that context. Luckily, since
                            // `Arc<Set>` has a fallback for when current::get() is None, we can
                            // just let the task run until it yields, and then put it back into the
                            // pool.

                            // We know that the code we're about to execute (inside
                            // Entry::run_task) has no way to reach the park passed to entry.run.
                            // therefore, it's fine for us to take ownership of it here _as long as
                            // we don't drop `park` later_! The DropNotGone wrapper around `park`
                            // takes care of that.
                            let park = unsafe { Box::from_raw(park_ptr) };
                            let worker = Worker {
                                entry: unsafe {
                                    // The same argument applies here. Since we unset `current`,
                                    // the task's execution won't assume that it owns a worker any
                                    // more. When the task yields, entry will use its `Arc<Set>`
                                    // (which is fine and safe), and then immediately return,
                                    // without calling any code that assumes there is only one
                                    // Entry with the given index (namely it won't call
                                    // Entry::owned).
                                    Entry::new(Arc::clone(&pool), index)
                                },
                                park,
                                launch_worker: Arc::clone(launch_worker),
                                gone: Cell::new(false),
                            };

                            // Give away the worker
                            //
                            // TODO: it would be _really_ nice if we had a way to _not_ spawn a
                            // thread and hand off the worker if the blocking routine ran only for
                            // a short amount of time. maybe push the Worker onto a "stealing
                            // queue" somehow? or maybe keep a shared "active" AtomicBool in both
                            // instances of the Worker, and compare_exchange it to true afterwards
                            // in an attempt to take it back. if it succeeds, we just resume where
                            // we were. if it fails, another thread has already stolen the Worker.
                            crate::executor::blocking::Pool::spawn(
                                &pool.blocking,
                                launch_worker(worker),
                            );

                            // make sure no subsequent code thinks that it is on a worker
                            current::clear();

                            // and make sure that when Entry finishes running the current task,
                            // it immediately returns all the way up to the worker.
                            gone.set(true);
                        };
                        let allow_blocking: &mut dyn FnMut() = &mut allow_blocking;

                        ob.set(Some(unsafe {
                            // NOTE: We cannot use a safe cast to raw pointer here, since we are
                            // _also_ erasing the lifetime of these pointers. That is safe here,
                            // because we know that ob will set back to None before allow_blocking
                            // is dropped.
                            #[allow(clippy::useless_transmute)]
                            std::mem::transmute::<_, *mut dyn FnMut()>(allow_blocking)
                        }));

                        let _ = entry.run(&mut **park, gone);

                        // Ensure that we reset ob before allow_blocking is dropped.
                        drop(_reset);
                    });
                })
            })
        });

        if gone.get() {
            // Synchronize with the pool for load(Acquire) in is_closed to get up-to-date value.
            pool.wait_for_unlocked();
            if pool.is_closed() {
                // If the pool is shutting down, some other thread may be waiting to clean up after
                // the task that we were holding on to. If we completed that task, we did nothing
                // (because task.run() returned None), and so crucially we did not wait up any such
                // thread.
                //
                // So, we have to do that here.
                pool.notify_all();
            }
        }
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
        self.entry.tick(&mut *self.park, &self.gone);
    }
}

struct WorkerGone;

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

    fn run(
        &mut self,
        park: &mut impl Park<Unpark = P>,
        gone: &Cell<bool>,
    ) -> Result<(), WorkerGone> {
        while self.is_running() {
            if self.tick(park, gone)? {
                self.park(park);
            }
        }

        self.shutdown(park);
        Ok(())
    }

    fn is_running(&mut self) -> bool {
        self.owned().is_running.get()
    }

    /// Returns `true` if the worker needs to park
    fn tick(
        &mut self,
        park: &mut impl Park<Unpark = P>,
        gone: &Cell<bool>,
    ) -> Result<bool, WorkerGone> {
        // Process all pending tasks in the local queue.
        if !self.process_local_queue(park, gone)? {
            return Ok(false);
        }

        // No more **local** work to process, try transitioning to searching
        // in order to attempt to steal work from other workers.
        //
        // On `false`, the worker has entered the parked state
        if self.transition_to_searching() {
            // If `true` then work was found
            if self.search_for_work(gone)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Process all pending tasks in the local queue, occasionally checking the
    /// global queue, but never other worker local queues.
    ///
    /// Returns `false` if processing was interrupted due to the pool shutting
    /// down.
    fn process_local_queue(
        &mut self,
        park: &mut impl Park<Unpark = P>,
        gone: &Cell<bool>,
    ) -> Result<bool, WorkerGone> {
        debug_assert!(self.is_running());

        loop {
            let tick = self.tick_fetch_inc();

            let task = if tick % GLOBAL_POLL_INTERVAL == 0 {
                // Sleep light...
                self.park_light(park);

                // Perform regularly scheduled maintenance work.
                self.maintenance();

                if !self.is_running() {
                    return Ok(false);
                }

                // Check the global queue
                self.owned().work_queue.pop_global_first()
            } else {
                self.owned().work_queue.pop_local_first()
            };

            if let Some(task) = task {
                self.run_task(task, gone)?;
            } else {
                return Ok(true);
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

    fn search_for_work(&mut self, gone: &Cell<bool>) -> Result<bool, WorkerGone> {
        debug_assert!(self.is_searching());

        if let Some(task) = self.steal_work() {
            self.run_task(task, gone)?;
            Ok(true)
        } else {
            // Perform some routine work
            self.drain_tasks_pending_drop();
            Ok(false)
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

    fn run_task(&mut self, task: Task<Shared<P>>, gone: &Cell<bool>) -> Result<(), WorkerGone> {
        if self.is_searching() {
            self.transition_from_searching();
        }

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
                self.pool.schedule(task);
            }
            return Err(WorkerGone);
        }

        if let Some(task) = task {
            self.owned().submit_local_yield(task);
            self.set().notify_work();
        }
        Ok(())
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

struct DropNotGone<'a, T> {
    gone: &'a Cell<bool>,
    inner: Option<T>,
}

impl<'a, T> DropNotGone<'a, T> {
    fn new(inner: T, gone: &'a Cell<bool>) -> Self {
        DropNotGone {
            gone,
            inner: Some(inner),
        }
    }
}

impl<'a, T> Drop for DropNotGone<'a, T> {
    fn drop(&mut self) {
        if self.gone.get() {
            let inner = self.inner.take().unwrap();
            std::mem::forget(inner);
        }
    }
}

impl<'a, T> Deref for DropNotGone<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<'a, T> DerefMut for DropNotGone<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}
