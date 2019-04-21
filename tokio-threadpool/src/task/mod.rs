mod blocking;
mod blocking_state;
mod state;

pub(crate) use self::blocking::{Blocking, CanBlock};
use self::blocking_state::BlockingState;
use self::state::State;

use notifier::Notifier;
use pool::Pool;

use futures::executor::{self, Spawn};
use futures::{self, Async, Future};

use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicPtr, AtomicUsize};
use std::sync::Arc;
use std::{fmt, panic, ptr};

/// Harness around a future.
///
/// This also behaves as a node in the inbound work queue and the blocking
/// queue.
pub(crate) struct Task {
    /// Task lifecycle state
    state: AtomicUsize,

    /// Task blocking related state
    blocking: AtomicUsize,

    /// Next pointer in the queue of tasks pending blocking capacity.
    next_blocking: AtomicPtr<Task>,

    /// ID of the worker that polled this task first.
    ///
    /// This field can be a `Cell` because it's only accessed by the worker thread that is
    /// executing the task.
    ///
    /// The worker ID is represented by a `u32` rather than `usize` in order to save some space
    /// on 64-bit platforms.
    pub reg_worker: Cell<Option<u32>>,

    /// The key associated with this task in the `Slab` it was registered in.
    ///
    /// This field can be a `Cell` because it's only accessed by the worker thread that has
    /// registered the task.
    pub reg_index: Cell<usize>,

    /// Store the future at the head of the struct
    ///
    /// The future is dropped immediately when it transitions to Complete
    future: UnsafeCell<Option<Spawn<BoxFuture>>>,
}

#[derive(Debug)]
pub(crate) enum Run {
    Idle,
    Schedule,
    Complete,
}

type BoxFuture = Box<Future<Item = (), Error = ()> + Send + 'static>;

// ===== impl Task =====

impl Task {
    /// Create a new `Task` as a harness for `future`.
    pub fn new(future: BoxFuture) -> Task {
        // Wrap the future with an execution context.
        let task_fut = executor::spawn(future);

        Task {
            state: AtomicUsize::new(State::new().into()),
            blocking: AtomicUsize::new(BlockingState::new().into()),
            next_blocking: AtomicPtr::new(ptr::null_mut()),
            reg_worker: Cell::new(None),
            reg_index: Cell::new(0),
            future: UnsafeCell::new(Some(task_fut)),
        }
    }

    /// Create a fake `Task` to be used as part of the intrusive mpsc channel
    /// algorithm.
    fn stub() -> Task {
        let future = Box::new(futures::empty()) as BoxFuture;
        let task_fut = executor::spawn(future);

        Task {
            state: AtomicUsize::new(State::stub().into()),
            blocking: AtomicUsize::new(BlockingState::new().into()),
            next_blocking: AtomicPtr::new(ptr::null_mut()),
            reg_worker: Cell::new(None),
            reg_index: Cell::new(0),
            future: UnsafeCell::new(Some(task_fut)),
        }
    }

    /// Execute the task returning `Run::Schedule` if the task needs to be
    /// scheduled again.
    pub fn run(&self, unpark: &Arc<Notifier>) -> Run {
        use self::State::*;

        // Transition task to running state. At this point, the task must be
        // scheduled.
        let actual: State = self
            .state
            .compare_and_swap(Scheduled.into(), Running.into(), AcqRel)
            .into();

        match actual {
            Scheduled => {}
            _ => panic!("unexpected task state; {:?}", actual),
        }

        trace!(
            "Task::run; state={:?}",
            State::from(self.state.load(Relaxed))
        );

        // The transition to `Running` done above ensures that a lock on the
        // future has been obtained.
        let fut = unsafe { &mut (*self.future.get()) };

        // This block deals with the future panicking while being polled.
        //
        // If the future panics, then the drop handler must be called such that
        // `thread::panicking() -> true`. To do this, the future is dropped from
        // within the catch_unwind block.
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            struct Guard<'a>(&'a mut Option<Spawn<BoxFuture>>, bool);

            impl<'a> Drop for Guard<'a> {
                fn drop(&mut self) {
                    // This drops the future
                    if self.1 {
                        let _ = self.0.take();
                    }
                }
            }

            let mut g = Guard(fut, true);

            let ret =
                g.0.as_mut()
                    .unwrap()
                    .poll_future_notify(unpark, self as *const _ as usize);

            g.1 = false;

            ret
        }));

        match res {
            Ok(Ok(Async::Ready(_))) | Ok(Err(_)) | Err(_) => {
                trace!("    -> task complete");

                // The future has completed. Drop it immediately to free
                // resources and run drop handlers.
                //
                // The `Task` harness will stay around longer if it is contained
                // by any of the various queues.
                self.drop_future();

                // Transition to the completed state
                self.state.store(State::Complete.into(), Release);

                if let Err(panic_err) = res {
                    if let Some(ref f) = unpark.pool.config.panic_handler {
                        f(panic_err);
                    }
                }

                Run::Complete
            }
            Ok(Ok(Async::NotReady)) => {
                trace!("    -> not ready");

                // Attempt to transition from Running -> Idle, if successful,
                // then the task does not need to be scheduled again. If the CAS
                // fails, then the task has been unparked concurrent to running,
                // in which case it transitions immediately back to scheduled
                // and we return `true`.
                let prev: State = self
                    .state
                    .compare_and_swap(Running.into(), Idle.into(), AcqRel)
                    .into();

                match prev {
                    Running => Run::Idle,
                    Notified => {
                        self.state.store(Scheduled.into(), Release);
                        Run::Schedule
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Aborts this task.
    ///
    /// This is called when the threadpool shuts down and the task has already beed polled but not
    /// completed.
    pub fn abort(&self) {
        use self::State::*;

        let mut state = self.state.load(Acquire).into();

        loop {
            match state {
                Idle | Scheduled => {}
                Running | Notified | Complete | Aborted => {
                    // It is assumed that no worker threads are running so the task must be either
                    // in the idle or scheduled state.
                    panic!("unexpected state while aborting task: {:?}", state);
                }
            }

            let actual = self
                .state
                .compare_and_swap(state.into(), Aborted.into(), AcqRel)
                .into();

            if actual == state {
                // The future has been aborted. Drop it immediately to free resources and run drop
                // handlers.
                self.drop_future();
                break;
            }

            state = actual;
        }
    }

    /// Notify the task
    pub fn notify(me: Arc<Task>, pool: &Arc<Pool>) {
        if me.schedule() {
            let _ = pool.submit(me, pool);
        }
    }

    /// Notify the task it has been allocated blocking capacity
    pub fn notify_blocking(me: Arc<Task>, pool: &Arc<Pool>) {
        BlockingState::notify_blocking(&me.blocking, AcqRel);
        Task::notify(me, pool);
    }

    /// Transition the task state to scheduled.
    ///
    /// Returns `true` if the caller is permitted to schedule the task.
    pub fn schedule(&self) -> bool {
        use self::State::*;

        loop {
            // Scheduling can only be done from the `Idle` state.
            let actual = self
                .state
                .compare_and_swap(Idle.into(), Scheduled.into(), AcqRel)
                .into();

            match actual {
                Idle => return true,
                Running => {
                    // The task is already running on another thread. Transition
                    // the state to `Notified`. If this CAS fails, then restart
                    // the logic again from `Idle`.
                    let actual = self
                        .state
                        .compare_and_swap(Running.into(), Notified.into(), AcqRel)
                        .into();

                    match actual {
                        Idle => continue,
                        _ => return false,
                    }
                }
                Complete | Aborted | Notified | Scheduled => return false,
            }
        }
    }

    /// Consumes any allocated capacity to block.
    ///
    /// Returns `true` if capacity was allocated, `false` otherwise.
    pub fn consume_blocking_allocation(&self) -> CanBlock {
        // This flag is the primary point of coordination. The queued flag
        // happens "around" setting the blocking capacity.
        BlockingState::consume_allocation(&self.blocking, AcqRel)
    }

    /// Drop the future
    ///
    /// This must only be called by the thread that successfully transitioned
    /// the future state to `Running`.
    fn drop_future(&self) {
        let _ = unsafe { (*self.future.get()).take() };
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Task")
            .field("state", &self.state)
            .field("future", &"Spawn<BoxFuture>")
            .finish()
    }
}
