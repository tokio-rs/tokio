mod queue;
mod state;

pub(crate) use self::queue::{Queue, Poll};
use self::state::State;

use notifier::Notifier;
use sender::Sender;

use futures::{self, Future, Async};
use futures::executor::{self, Spawn};

use std::{fmt, panic, ptr};
use std::cell::{UnsafeCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicPtr};
use std::sync::atomic::Ordering::{AcqRel, Release, Relaxed};

#[cfg(feature = "unstable-futures")]
use futures2;

/// Harness around a future.
///
/// This also behaves as a node in the inbound work queue and the blocking
/// queue.
pub(crate) struct Task {
    /// Task state
    state: AtomicUsize,

    /// Next pointer in the queue that submits tasks to a worker.
    next: AtomicPtr<Task>,

    /// Store the future at the head of the struct
    ///
    /// The future is dropped immediately when it transitions to Complete
    future: UnsafeCell<Option<TaskFuture>>,
}

#[derive(Debug)]
pub(crate) enum Run {
    Idle,
    Schedule,
    Complete,
}

type BoxFuture = Box<Future<Item = (), Error = ()> + Send + 'static>;

#[cfg(feature = "unstable-futures")]
type BoxFuture2 = Box<futures2::Future<Item = (), Error = futures2::Never> + Send>;

enum TaskFuture {
    Futures1(Spawn<BoxFuture>),

    #[cfg(feature = "unstable-futures")]
    Futures2 {
        tls: futures2::task::LocalMap,
        waker: futures2::task::Waker,
        fut: BoxFuture2,
    }
}

// ===== impl Task =====

impl Task {
    /// Create a new `Task` as a harness for `future`.
    pub fn new(future: BoxFuture) -> Task {
        // Wrap the future with an execution context.
        let task_fut = TaskFuture::Futures1(executor::spawn(future));

        Task {
            state: AtomicUsize::new(State::new().into()),
            next: AtomicPtr::new(ptr::null_mut()),
            future: UnsafeCell::new(Some(task_fut)),
        }
    }

    /// Create a new `Task` as a harness for a futures 0.2 `future`.
    #[cfg(feature = "unstable-futures")]
    pub fn new2<F>(fut: BoxFuture2, make_waker: F) -> Task
    where F: FnOnce(usize) -> futures2::task::Waker
    {
        let mut inner = Box::new(Task {
            next: AtomicPtr::new(ptr::null_mut()),
            state: AtomicUsize::new(State::new().into()),
            future: None,
        });

        let waker = make_waker((&*inner) as *const _ as usize);
        let tls = futures2::task::LocalMap::new();
        inner.future = Some(TaskFuture::Futures2 { waker, tls, fut });

        Task { ptr: Box::into_raw(inner) }
    }

    /// Create a fake `Task` to be used as part of the intrusive mpsc channel
    /// algorithm.
    fn stub() -> Task {
        let future = Box::new(futures::empty());
        let task_fut = TaskFuture::Futures1(executor::spawn(future));

        Task {
            next: AtomicPtr::new(ptr::null_mut()),
            state: AtomicUsize::new(State::stub().into()),
            future: UnsafeCell::new(Some(task_fut)),
        }
    }

    /// Execute the task returning `Run::Schedule` if the task needs to be
    /// scheduled again.
    pub fn run(&self, unpark: &Arc<Notifier>, exec: &mut Sender) -> Run {
        use self::State::*;

        // Transition task to running state. At this point, the task must be
        // scheduled.
        let actual: State = self.state.compare_and_swap(
            Scheduled.into(), Running.into(), AcqRel).into();

        trace!("running; state={:?}", actual);

        match actual {
            Scheduled => {},
            _ => panic!("unexpected task state; {:?}", actual),
        }

        trace!("Task::run; state={:?}", State::from(self.state.load(Relaxed)));

        // The transition to `Running` done above ensures that a lock on the
        // future has been obtained.
        let fut = unsafe { &mut (*self.future.get()) };

        // This block deals with the future panicking while being polled.
        //
        // If the future panics, then the drop handler must be called such that
        // `thread::panicking() -> true`. To do this, the future is dropped from
        // within the catch_unwind block.
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            struct Guard<'a>(&'a mut Option<TaskFuture>, bool);

            impl<'a> Drop for Guard<'a> {
                fn drop(&mut self) {
                    // This drops the future
                    if self.1 {
                        let _ = self.0.take();
                    }
                }
            }

            let mut g = Guard(fut, true);

            let ret = g.0.as_mut().unwrap()
                .poll(unpark, self as *const _ as usize, exec);


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

                Run::Complete
            }
            Ok(Ok(Async::NotReady)) => {
                trace!("    -> not ready");

                // Attempt to transition from Running -> Idle, if successful,
                // then the task does not need to be scheduled again. If the CAS
                // fails, then the task has been unparked concurrent to running,
                // in which case it transitions immediately back to scheduled
                // and we return `true`.
                let prev: State = self.state.compare_and_swap(
                    Running.into(), Idle.into(), AcqRel).into();

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

    /// Transition the task state to scheduled.
    ///
    /// Returns `true` if the caller is permitted to schedule the task.
    pub fn schedule(&self) -> bool {
        use self::State::*;

        loop {
            // Scheduling can only be done from the `Idle` state.
            let actual = self.state.compare_and_swap(
                Idle.into(),
                Scheduled.into(),
                AcqRel).into();

            match actual {
                Idle => return true,
                Running => {
                    // The task is already running on another thread. Transition
                    // the state to `Notified`. If this CAS fails, then restart
                    // the logic again from `Idle`.
                    let actual = self.state.compare_and_swap(
                        Running.into(), Notified.into(), AcqRel).into();

                    match actual {
                        Idle => continue,
                        _ => return false,
                    }
                }
                Complete | Notified | Scheduled => return false,
            }
        }
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
            .field("next", &self.next)
            .field("state", &self.state)
            .field("future", &"Spawn<BoxFuture>")
            .finish()
    }
}

// ===== impl TaskFuture =====

impl TaskFuture {
    #[allow(unused_variables)]
    fn poll(&mut self, unpark: &Arc<Notifier>, id: usize, exec: &mut Sender) -> futures::Poll<(), ()> {
        match *self {
            TaskFuture::Futures1(ref mut fut) => fut.poll_future_notify(unpark, id),

            #[cfg(feature = "unstable-futures")]
            TaskFuture::Futures2 { ref mut fut, ref waker, ref mut tls } => {
                let mut cx = futures2::task::Context::new(tls, waker, exec);
                match fut.poll(&mut cx).unwrap() {
                    futures2::Async::Pending => Ok(Async::NotReady),
                    futures2::Async::Ready(x) => Ok(Async::Ready(x)),
                }
            }
        }
    }
}
