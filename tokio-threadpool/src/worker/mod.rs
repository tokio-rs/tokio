mod entry;
mod state;

pub(crate) use self::entry::{
    WorkerEntry as Entry,
};
pub(crate) use self::state::{
    State,
    Lifecycle,
};

use pool::{self, Pool};
use notifier::Notifier;
use sender::Sender;
use task::Task;

use tokio_executor;

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use std::sync::Arc;
use std::thread;
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

    // Set when the worker should finalize on drop
    should_finalize: Cell<bool>,

    // Keep the value on the current thread.
    _p: PhantomData<Rc<()>>,
}

/// Identifiers a thread pool worker.
///
/// This identifier is unique scoped by the thread pool. It is possible that
/// different thread pool instances share worker identifier values.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct WorkerId {
    pub(crate) idx: usize,
}

// Pointer to the current worker info
thread_local!(static CURRENT_WORKER: Cell<*const Worker> = Cell::new(0 as *const _));

impl Worker {
    pub(crate) fn spawn(id: WorkerId, inner: &Arc<Pool>) {
        trace!("spawning new worker thread; id={}", id.idx);

        let mut th = thread::Builder::new();

        if let Some(ref prefix) = inner.config.name_prefix {
            th = th.name(format!("{}{}", prefix, id.idx));
        }

        if let Some(stack) = inner.config.stack_size {
            th = th.stack_size(stack);
        }

        let inner = inner.clone();

        th.spawn(move || {
            let worker = Worker {
                inner,
                id,
                should_finalize: Cell::new(false),
                _p: PhantomData,
            };

            // Make sure the ref to the worker does not move
            let wref = &worker;

            // Create another worker... It's ok, this is just a new type around
            // `Pool` that is expected to stay on the current thread.
            CURRENT_WORKER.with(|c| {
                c.set(wref as *const _);

                let inner = wref.inner.clone();
                let mut sender = Sender { inner };

                // Enter an execution context
                let mut enter = tokio_executor::enter().unwrap();

                tokio_executor::with_default(&mut sender, &mut enter, |enter| {
                    if let Some(ref callback) = wref.inner.config.around_worker {
                        callback.call(wref, enter);
                    } else {
                        wref.run();
                    }
                });
            });
        }).unwrap();
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
                if tick % LIGHT_SLEEP_INTERVAL == 0 {
                    self.sleep_light();
                }

                tick = tick.wrapping_add(1);
                spin_cnt = 0;

                // As long as there is work, keep looping.
                continue;
            }

            // No work in this worker's queue, it is time to try stealing.
            if self.try_steal_task(&notify, &mut sender) {
                if tick % LIGHT_SLEEP_INTERVAL == 0 {
                    self.sleep_light();
                }

                tick = tick.wrapping_add(1);
                spin_cnt = 0;
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

        self.should_finalize.set(true);
    }

    /// Checks the worker's current state, updating it as needed.
    ///
    /// Returns `true` if the worker should run.
    #[inline]
    fn check_run_state(&self, first: bool) -> bool {
        use self::Lifecycle::*;

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
    #[inline]
    fn try_run_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
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
    #[inline]
    fn try_steal_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
        use deque::Steal::*;

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
                               self.id.idx, idx);

                        // Signal other workers that work is available
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

    fn run_task(&self, task: Task, notify: &Arc<Notifier>, sender: &mut Sender) {
        use task::Run::*;

        match task.run(notify, sender) {
            Idle => {}
            Schedule => {
                self.entry().push_internal(task);
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

        trace!("Worker::sleep; worker={:?}", self);

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

                    trace!("  sleeping -- push to stack; idx={}", self.id.idx);

                    // We obtained permission to push the worker into the
                    // sleeper queue.
                    if let Err(_) = self.inner.push_sleeper(self.id.idx) {
                        trace!("  sleeping -- push to stack failed; idx={}", self.id.idx);
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

        trace!("    -> starting to sleep; idx={}", self.id.idx);

        let sleep_until = self.inner.config.keep_alive
            .map(|dur| Instant::now() + dur);

        // The state has been transitioned to sleeping, we can now wait by
        // calling the parker. This is done in a loop as condvars can wakeup
        // spuriously.
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

            trace!("    -> wakeup; idx={}", self.id.idx);

            // Reload the state
            state = self.entry().state.load(Acquire).into();

            loop {
                match state.lifecycle() {
                    Sleeping => {}
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

                if !drop_thread {
                    // This goees back to the outer loop.
                    break;
                }

                let mut next = state;
                next.set_lifecycle(Shutdown);

                let actual = self.entry().state.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual == state {
                    // Transitioned to a shutdown state
                    return false;
                }

                state = actual;
            }

            // The worker hasn't been notified, go back to sleep
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
        &self.inner.workers[self.id.idx]
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        trace!("shutting down thread; idx={}", self.id.idx);

        if self.should_finalize.get() {
            // Get all inbound work and push it onto the work queue. The work
            // queue is drained in the next step.
            self.drain_inbound();

            // Drain the work queue
            self.entry().drain_tasks();

            // TODO: Drain the work queue...
            self.inner.worker_terminated();
        }
    }
}

impl WorkerId {
    pub(crate) fn new(idx: usize) -> WorkerId {
        WorkerId { idx }
    }
}
