mod entry;
mod stack;
mod state;

pub(crate) use self::entry::{
    WorkerEntry as Entry,
};
pub(crate) use self::stack::Stack;
pub(crate) use self::state::{
    State,
    Lifecycle,
};

use pool::{self, Pool, BackupId};
use notifier::Notifier;
use sender::Sender;
use task::{self, Task, CanBlock};

use tokio_executor;

use futures::{Poll, Async};

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Thread worker
///
/// This is passed to the `around_worker` callback set on `Builder`. This
/// callback is only expected to call `run` on it.
#[derive(Debug)]
pub struct Worker {
    // Shared scheduler data
    pub(crate) inner: Arc<Pool>,

    // WorkerEntry index
    pub(crate) id: WorkerId,

    // Backup thread ID assigned to processing this worker.
    backup_id: BackupId,

    // Set to the task that is currently being polled by the worker. This is
    // needed so that `blocking` blocks are able to interact with this task.
    //
    // This has to be a raw pointer to make it compile, but great care is taken
    // when this is set.
    current_task: CurrentTask,

    // Set when the thread is in blocking mode.
    is_blocking: Cell<bool>,

    // Set when the worker should finalize on drop
    should_finalize: Cell<bool>,

    // Keep the value on the current thread.
    _p: PhantomData<Rc<()>>,
}

/// Tracks the state related to the currently running task.
#[derive(Debug)]
struct CurrentTask {
    /// This has to be a raw pointer to make it compile, but great care is taken
    /// when this is set.
    task: Cell<Option<*const Arc<Task>>>,

    /// Tracks the blocking capacity allocation state.
    can_block: Cell<CanBlock>,
}

/// Identifiers a thread pool worker.
///
/// This identifier is unique scoped by the thread pool. It is possible that
/// different thread pool instances share worker identifier values.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct WorkerId(pub(crate) usize);

// Pointer to the current worker info
thread_local!(static CURRENT_WORKER: Cell<*const Worker> = Cell::new(0 as *const _));

impl Worker {
    pub(crate) fn new(id: WorkerId, backup_id: BackupId, inner: Arc<Pool>) -> Worker {
        Worker {
            inner,
            id,
            backup_id,
            current_task: CurrentTask::new(),
            is_blocking: Cell::new(false),
            should_finalize: Cell::new(false),
            _p: PhantomData,
        }
    }

    pub(crate) fn is_blocking(&self) -> bool {
        self.is_blocking.get()
    }

    /// Run the worker
    ///
    /// Returns `true` if the thread should keep running as a `backup` thread.
    pub(crate) fn do_run(&self) -> bool {
        // Create another worker... It's ok, this is just a new type around
        // `Pool` that is expected to stay on the current thread.
        CURRENT_WORKER.with(|c| {
            c.set(self as *const _);

            let inner = self.inner.clone();
            let mut sender = Sender { inner };

            // Enter an execution context
            let mut enter = tokio_executor::enter().unwrap();

            tokio_executor::with_default(&mut sender, &mut enter, |enter| {
                if let Some(ref callback) = self.inner.config.around_worker {
                    callback.call(self, enter);
                } else {
                    self.run();
                }
            });
        });

        // Can't be in blocking mode and finalization mode
        debug_assert!(!self.is_blocking.get() || !self.should_finalize.get());

        self.is_blocking.get()
    }

    pub(crate) fn with_current<F: FnOnce(Option<&Worker>) -> R, R>(f: F) -> R {
        CURRENT_WORKER.with(move |c| {
            let ptr = c.get();

            if ptr.is_null() {
                f(None)
            } else {
                f(Some(unsafe { &*ptr }))
            }
        })
    }

    /// Transition the current worker to a blocking worker
    pub(crate) fn transition_to_blocking(&self) -> Poll<(), ::BlockingError> {
        use self::CanBlock::*;

        // If we get this far, then `current_task` has been set.
        let task_ref = self.current_task.get_ref();

        // First step is to acquire blocking capacity for the task.
        match self.current_task.can_block() {
            // Capacity to block has already been allocated to this task.
            Allocated => {}

            // The task has already requested capacity to block, but there is
            // none yet available.
            NoCapacity => return Ok(Async::NotReady),

            // The task has yet to ask for capacity
            CanRequest => {
                // Atomically attempt to acquire blocking capacity, and if none
                // is available, register the task to be notified once capacity
                // becomes available.
                match self.inner.poll_blocking_capacity(task_ref)? {
                    Async::Ready(()) => {
                        self.current_task.set_can_block(Allocated);
                    }
                    Async::NotReady => {
                        self.current_task.set_can_block(NoCapacity);
                        return Ok(Async::NotReady);
                    }
                }
            }
        }

        // The task has been allocated blocking capacity. At this point, this is
        // when the current thread transitions from a worker to a backup thread.
        // To do so requires handing over the worker to another backup thread.

        if self.is_blocking.get() {
            // The thread is already in blocking mode, so there is nothing else
            // to do. Return `Ready` and allow the caller to block the thread.
            return Ok(().into());
        }

        trace!("transition to blocking state");

        // Transitioning to blocking requires handing over the worker state to
        // another thread so that the work queue can continue to be processed.

        self.inner.spawn_thread(self.id.clone(), &self.inner);

        // Track that the thread has now fully entered the blocking state.
        self.is_blocking.set(true);

        Ok(().into())
    }

    /// Transition from blocking
    pub(crate) fn transition_from_blocking(&self) {
        // TODO: Attempt to take ownership of the worker again.
    }

    /// Returns a reference to the worker's identifier.
    ///
    /// This identifier is unique scoped by the thread pool. It is possible that
    /// different thread pool instances share worker identifier values.
    pub fn id(&self) -> &WorkerId {
        &self.id
    }

    /// Run the worker
    ///
    /// This function blocks until the worker is shutting down.
    pub fn run(&self) {
        const LIGHT_SLEEP_INTERVAL: usize = 32;

        // Get the notifier.
        let notify = Arc::new(Notifier {
            inner: Arc::downgrade(&self.inner),
        });
        let mut sender = Sender { inner: self.inner.clone() };

        let mut first = true;
        let mut spin_cnt = 0;
        let mut tick = 0;

        while self.check_run_state(first) {
            first = false;

            // Poll inbound until empty, transfering all tasks to the internal
            // queue.
            let consistent = self.drain_inbound();

            // Run the next available task
            if self.try_run_task(&notify, &mut sender) {
                if self.is_blocking.get() {
                    // Exit out of the run state
                    return;
                }

                if tick % LIGHT_SLEEP_INTERVAL == 0 {
                    self.sleep_light();
                }

                tick = tick.wrapping_add(1);
                spin_cnt = 0;

                // As long as there is work, keep looping.
                continue;
            }

            if !consistent {
                spin_cnt = 0;
                continue;
            }

            // Starting to get sleeeeepy
            if spin_cnt < 61 {
                spin_cnt += 1;
            } else {
                tick = 0;

                if !self.sleep() {
                    return;
                }
            }

            // If there still isn't any work to do, shutdown the worker?
        }

        // The pool is terminating. However, transitioning the pool state to
        // terminated is the very first step of the finalization process. Other
        // threads may not see this state and try to spawn a new thread. To
        // ensure consistency, before the current thread shuts down, it must
        // return the backup token to the stack.
        //
        // The returned result is ignored because `Err` represents the pool
        // shutting down. We are currently aware of this fact.
        let _ = self.inner.release_backup(self.backup_id);

        self.should_finalize.set(true);
    }

    /// Try to run a task
    ///
    /// Returns `true` if work was found.
    #[inline]
    fn try_run_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
        if self.try_run_owned_task(notify, sender) {
            return true;
        }

        self.try_steal_task(notify, sender)
    }

    /// Checks the worker's current state, updating it as needed.
    ///
    /// Returns `true` if the worker should run.
    #[inline]
    fn check_run_state(&self, first: bool) -> bool {
        use self::Lifecycle::*;

        debug_assert!(!self.is_blocking.get());

        let mut state: State = self.entry().state.load(Acquire).into();

        loop {
            let pool_state: pool::State = self.inner.state.load(Acquire).into();

            if pool_state.is_terminated() {
                return false;
            }

            let mut next = state;

            match state.lifecycle() {
                Running => break,
                Notified | Signaled => {
                    // transition back to running
                    next.set_lifecycle(Running);
                }
                Shutdown | Sleeping => {
                    // The worker should never be in these states when calling
                    // this function.
                    panic!("unexpected worker state; lifecycle={:?}", state.lifecycle());
                }
            }

            let actual = self.entry().state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                break;
            }

            state = actual;
        }

        // If this is the first iteration of the worker loop, then the state can
        // be signaled.
        if !first && state.is_signaled() {
            trace!("Worker::check_run_state; delegate signal");
            // This worker is not ready to be signaled, so delegate the signal
            // to another worker.
            self.inner.signal_work(&self.inner);
        }

        true
    }

    /// Runs the next task on this worker's queue.
    ///
    /// Returns `true` if work was found.
    fn try_run_owned_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
        use deque::Steal::*;

        // Poll the internal queue for a task to run
        match self.entry().pop_task() {
            Data(task) => {
                self.run_task(task, notify, sender);
                true
            }
            Empty => false,
            Retry => true,
        }
    }

    /// Tries to steal a task from another worker.
    ///
    /// Returns `true` if work was found
    fn try_steal_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
        use deque::Steal::*;

        debug_assert!(!self.is_blocking.get());

        let len = self.inner.workers.len();
        let mut idx = self.inner.rand_usize() % len;
        let mut found_work = false;
        let start = idx;

        loop {
            if idx < len {
                match self.inner.workers[idx].steal_task() {
                    Data(task) => {
                        trace!("stole task");

                        self.run_task(task, notify, sender);

                        trace!("try_steal_task -- signal_work; self={}; from={}",
                               self.id.0, idx);

                        // Signal other workers that work is available
                        //
                        // TODO: Should this be called here or before
                        // `run_task`?
                        self.inner.signal_work(&self.inner);

                        return true;
                    }
                    Empty => {}
                    Retry => found_work = true,
                }

                idx += 1;
            } else {
                idx = 0;
            }

            if idx == start {
                break;
            }
        }

        found_work
    }

    fn run_task(&self, task: Arc<Task>, notify: &Arc<Notifier>, sender: &mut Sender) {
        use task::Run::*;

        let run = self.run_task2(&task, notify, sender);

        // TODO: Try to claim back the worker state in case the backup thread
        // did not start up fast enough. This is a performance optimization.

        match run {
            Idle => {}
            Schedule => {
                if self.is_blocking.get() {
                    // The future has been notified while it was running.
                    // However, the future also entered a blocking section,
                    // which released the worker state from this thread.
                    //
                    // This means that scheduling the future must be done from
                    // a point of view external to the worker set.
                    //
                    // We have to call `submit_external` instead of `submit`
                    // here because `self` is still set as the current worker.
                    self.inner.submit_external(task, &self.inner);
                } else {
                    self.entry().push_internal(task);
                }
            }
            Complete => {
                let mut state: pool::State = self.inner.state.load(Acquire).into();

                loop {
                    let mut next = state;
                    next.dec_num_futures();

                    let actual = self.inner.state.compare_and_swap(
                        state.into(), next.into(), AcqRel).into();

                    if actual == state {
                        trace!("task complete; state={:?}", next);

                        if state.num_futures() == 1 {
                            // If the thread pool has been flagged as shutdown,
                            // start terminating workers. This involves waking
                            // up any sleeping worker so that they can notice
                            // the shutdown state.
                            if next.is_terminated() {
                                self.inner.terminate_sleeping_workers();
                            }
                        }

                        // The worker's run loop will detect the shutdown state
                        // next iteration.
                        return;
                    }

                    state = actual;
                }
            }
        }
    }

    /// Actually run the task. This is where `Worker::current_task` is set.
    ///
    /// Great care is needed to ensure that `current_task` is unset in this
    /// function.
    fn run_task2(&self,
                 task: &Arc<Task>,
                 notify: &Arc<Notifier>,
                 sender: &mut Sender)
        -> task::Run
    {
        struct Guard<'a> {
            worker: &'a Worker,
            allocated_at_run: bool
        }

        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                // A task is allocated at run when it was explicitly notified
                // that the task has capacity to block. When this happens, that
                // capacity is automatically allocated to the notified task.
                // This capacity is "use it or lose it", so if the thread is not
                // transitioned to blocking in this call, then another task has
                // to be notified.
                if self.allocated_at_run && !self.worker.is_blocking.get() {
                    self.worker.inner.notify_blocking_task(&self.worker.inner);
                }

                self.worker.current_task.clear();
            }
        }

        let can_block = task.consume_blocking_allocation();

        // Set `current_task`
        self.current_task.set(task, can_block);

        // Create the guard, this ensures that `current_task` is unset when the
        // function returns, even if the return is caused by a panic.
        let _g = Guard {
            worker: self,
            allocated_at_run: can_block == CanBlock::Allocated
        };

        task.run(notify, sender)
    }

    /// Drains all tasks on the extern queue and pushes them onto the internal
    /// queue.
    ///
    /// Returns `true` if the operation was able to complete in a consistent
    /// state.
    #[inline]
    fn drain_inbound(&self) -> bool {
        use task::Poll::*;

        let mut found_work = false;

        loop {
            let task = unsafe { self.entry().inbound.poll() };

            match task {
                Empty => {
                    if found_work {
                        trace!("found work while draining; signal_work");
                        self.inner.signal_work(&self.inner);
                    }

                    return true;
                }
                Inconsistent => {
                    if found_work {
                        trace!("found work while draining; signal_work");
                        self.inner.signal_work(&self.inner);
                    }

                    return false;
                }
                Data(task) => {
                    found_work = true;
                    self.entry().push_internal(task);
                }
            }
        }
    }

    /// Put the worker to sleep
    ///
    /// Returns `true` if woken up due to new work arriving.
    fn sleep(&self) -> bool {
        use self::Lifecycle::*;

        // Putting a worker to sleep is a multipart operation. This is, in part,
        // due to the fact that a worker can be notified without it being popped
        // from the sleep stack. Extra care is needed to deal with this.

        trace!("Worker::sleep; worker={:?}", self.id);

        let mut state: State = self.entry().state.load(Acquire).into();

        // The first part of the sleep process is to transition the worker state
        // to "pushed". Now, it may be that the worker is already pushed on the
        // sleeper stack, in which case, we don't push again.

        loop {
            let mut next = state;

            match state.lifecycle() {
                Running => {
                    // Try setting the pushed state
                    next.set_pushed();

                    // Transition the worker state to sleeping
                    next.set_lifecycle(Sleeping);
                }
                Notified | Signaled => {
                    // No need to sleep, transition back to running and move on.
                    next.set_lifecycle(Running);
                }
                Shutdown | Sleeping => {
                    // The worker cannot transition to sleep when already in a
                    // sleeping state.
                    panic!("unexpected worker state; actual={:?}", state.lifecycle());
                }
            }

            let actual = self.entry().state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                if state.is_notified() {
                    // The previous state was notified, so we don't need to
                    // sleep.
                    return true;
                }

                if !state.is_pushed() {
                    debug_assert!(next.is_pushed());

                    trace!("  sleeping -- push to stack; idx={}", self.id.0);

                    // We obtained permission to push the worker into the
                    // sleeper queue.
                    if let Err(_) = self.inner.push_sleeper(self.id.0) {
                        trace!("  sleeping -- push to stack failed; idx={}", self.id.0);
                        // The push failed due to the pool being terminated.
                        //
                        // This is true because the "work" being woken up for is
                        // shutting down.
                        return true;
                    }
                }

                break;
            }

            state = actual;
        }

        trace!("    -> starting to sleep; idx={}", self.id.0);

        let sleep_until = self.inner.config.keep_alive
            .map(|dur| Instant::now() + dur);

        // The state has been transitioned to sleeping, we can now wait by
        // calling the parker. This is done in a loop as condvars can wakeup
        // spuriously.
        'sleep:
        loop {
            let mut drop_thread = false;

            match sleep_until {
                Some(when) => {
                    let now = Instant::now();

                    if when >= now {
                        drop_thread = true;
                    }

                    let dur = when - now;

                    unsafe {
                        (*self.entry().park.get())
                            .park_timeout(dur)
                            .unwrap();
                    }
                }
                None => {
                    unsafe {
                        (*self.entry().park.get())
                            .park()
                            .unwrap();
                    }
                }
            }

            trace!("    -> wakeup; idx={}", self.id.0);

            // Reload the state
            state = self.entry().state.load(Acquire).into();

            // If the worker has been notified, transition back to running.
            match state.lifecycle() {
                Sleeping => {
                    if !drop_thread {
                        // This goes back to the outer loop.
                        continue 'sleep;
                    }
                }
                Notified | Signaled => {
                    // Transition back to running
                    loop {
                        let mut next = state;
                        next.set_lifecycle(Running);

                        let actual = self.entry().state.compare_and_swap(
                            state.into(), next.into(), AcqRel).into();

                        if actual == state {
                            return true;
                        }

                        state = actual;
                    }
                }
                Shutdown | Running => {
                    // To get here, the block above transitioned the tate to
                    // `Sleeping`. No other thread can concurrently
                    // transition to `Shutdown` or `Running`.
                    unreachable!();
                }
            }

            // The thread has reached the maximum permitted sleep duration.
            // It is now going to begin to shutdown.
            //
            // Doing this requires first releasing the thread to the backup
            // stack. Because the moment the worker state is transitioned to
            // `Shutdown`, other threads **expect** the thread's backup
            // entry to be available on the backup stack.
            //
            // However, it is possible that the worker is notified between
            // us pushing the backup entry onto the backup stack and
            // transitioning the worker to `Shutdown`. If this happens, the
            // current thread lost the token to run the backup entry and has
            // to shutdown no matter what.
            //
            // To deal with this, the worker is transitioned to another
            // thread. This is a pretty rare condition.
            //
            // If pushing on the backup stack fails, then the pool is being
            // terminated and the thread should just shutdown
            let backup_push_err = self.inner.release_backup(self.backup_id).is_err();

            if backup_push_err {
                debug_assert!({
                    let state: State = self.entry().state.load(Acquire).into();
                    state.lifecycle() != Sleeping
                });

                self.should_finalize.set(true);

                return true;
            }

            loop {
                let mut next = state;
                next.set_lifecycle(Shutdown);

                let actual: State = self.entry().state.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual == state {
                    // Transitioned to a shutdown state
                    return false;
                }

                match actual.lifecycle() {
                    Sleeping => {
                        state = actual;
                    }
                    Notified | Signaled => {
                        // Transition back to running
                        loop {
                            let mut next = state;
                            next.set_lifecycle(Running);

                            let actual = self.entry().state.compare_and_swap(
                                state.into(), next.into(), AcqRel).into();

                            if actual == state {
                                self.inner.spawn_thread(self.id.clone(), &self.inner);
                                return false;
                            }

                            state = actual;
                        }
                    }
                    Shutdown | Running => {
                        // To get here, the block above transitioned the tate to
                        // `Sleeping`. No other thread can concurrently
                        // transition to `Shutdown` or `Running`.
                        unreachable!();
                    }
                }
            }
        }
    }

    /// This doesn't actually put the thread to sleep. It calls
    /// `park.park_timeout` with a duration of 0. This allows the park
    /// implementation to perform any work that might be done on an interval.
    fn sleep_light(&self) {
        unsafe {
            (*self.entry().park.get())
                .park_timeout(Duration::from_millis(0))
                .unwrap();
        }
    }

    fn entry(&self) -> &Entry {
        debug_assert!(!self.is_blocking.get());
        &self.inner.workers[self.id.0]
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        trace!("shutting down thread; idx={}", self.id.0);

        if self.should_finalize.get() {
            // Get all inbound work and push it onto the work queue. The work
            // queue is drained in the next step.
            self.drain_inbound();

            // Drain the work queue
            self.entry().drain_tasks();

            // TODO: Drain the work queue...
        }
    }
}

// ===== impl CurrentTask =====

impl CurrentTask {
    /// Returns a default `CurrentTask` representing no task.
    fn new() -> CurrentTask {
        CurrentTask {
            task: Cell::new(None),
            can_block: Cell::new(CanBlock::CanRequest),
        }
    }

    /// Returns a reference to the task.
    fn get_ref(&self) -> &Arc<Task> {
        unsafe { &*self.task.get().unwrap() }
    }

    fn can_block(&self) -> CanBlock {
        self.can_block.get()
    }

    fn set_can_block(&self, can_block: CanBlock) {
        self.can_block.set(can_block);
    }

    fn set(&self, task: &Arc<Task>, can_block: CanBlock) {
        self.task.set(Some(task as *const _));
        self.can_block.set(can_block);
    }

    /// Reset the `CurrentTask` to null state.
    fn clear(&self) {
        self.task.set(None);
        self.can_block.set(CanBlock::CanRequest);
    }
}

// ===== impl WorkerId =====

impl WorkerId {
    /// Returns a `WorkerId` representing the worker entry at index `idx`.
    pub(crate) fn new(idx: usize) -> WorkerId {
        WorkerId(idx)
    }
}
