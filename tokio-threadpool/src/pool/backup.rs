use park::DefaultPark;
use worker::WorkerId;

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{self, AcqRel, Acquire, Relaxed};
use std::time::{Duration, Instant};

/// State associated with a thread in the thread pool.
///
/// The pool manages a number of threads. Some of those threads are considered
/// "primary" threads and process the work queue. When a task being run on a
/// primary thread enters a blocking context, the responsibility of processing
/// the work queue must be handed off to another thread. This is done by first
/// checking for idle threads on the backup stack. If one is found, the worker
/// token (`WorkerId`) is handed off to that running thread. If none are found,
/// a new thread is spawned.
///
/// This state manages the exchange. A thread that is idle, not assigned to a
/// work queue, sits around for a specified amount of time. When the worker
/// token is handed off, it is first stored in `handoff`. The backup thread is
/// then signaled. At this point, the backup thread wakes up from sleep and
/// reads `handoff`. At that point, it has been promoted to a primary thread and
/// will begin processing inbound work on the work queue.
///
/// The name `Backup` isn't really great for what the type does, but I have not
/// come up with a better name... Maybe it should just be named `Thread`.
#[derive(Debug)]
pub(crate) struct Backup {
    /// Worker ID that is being handed to this thread.
    handoff: UnsafeCell<Option<WorkerId>>,

    /// Thread state.
    ///
    /// This tracks:
    ///
    /// * Is queued flag
    /// * If the pool is shutting down.
    /// * If the thread is running
    state: AtomicUsize,

    /// Next entry in the Treiber stack.
    next_sleeper: UnsafeCell<BackupId>,

    /// Used to put the thread to sleep
    park: DefaultPark,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub(crate) struct BackupId(pub(crate) usize);

#[derive(Debug)]
pub(crate) enum Handoff {
    Worker(WorkerId),
    Idle,
    Terminated,
}

/// Tracks thread state.
#[derive(Clone, Copy, Eq, PartialEq)]
struct State(usize);

/// Set when the worker is pushed onto the scheduler's stack of sleeping
/// threads.
///
/// This flag also serves as a "notification" bit. If another thread is
/// attempting to hand off a worker to the backup thread, then the pushed bit
/// will not be set when the thread tries to shutdown.
pub const PUSHED: usize = 0b001;

/// Set when the thread is running
pub const RUNNING: usize = 0b010;

/// Set when the thread pool has terminated
pub const TERMINATED: usize = 0b100;

// ===== impl Backup =====

impl Backup {
    pub fn new() -> Backup {
        Backup {
            handoff: UnsafeCell::new(None),
            state: AtomicUsize::new(State::new().into()),
            next_sleeper: UnsafeCell::new(BackupId(0)),
            park: DefaultPark::new(),
        }
    }

    /// Called when the thread is starting
    pub fn start(&self, worker_id: &WorkerId) {
        debug_assert!({
            let state: State = self.state.load(Relaxed).into();

            debug_assert!(!state.is_pushed());
            debug_assert!(state.is_running());
            debug_assert!(!state.is_terminated());

            true
        });

        // The handoff value is equal to `worker_id`
        debug_assert_eq!(unsafe { (*self.handoff.get()).as_ref() }, Some(worker_id));

        unsafe {
            *self.handoff.get() = None;
        }
    }

    pub fn is_running(&self) -> bool {
        let state: State = self.state.load(Relaxed).into();
        state.is_running()
    }

    /// Hands off the worker to a thread.
    ///
    /// Returns `true` if the thread needs to be spawned.
    pub fn worker_handoff(&self, worker_id: WorkerId) -> bool {
        unsafe {
            // The backup worker should not already have been handoff a worker.
            debug_assert!((*self.handoff.get()).is_none());

            // Set the handoff
            *self.handoff.get() = Some(worker_id);
        }

        // This *probably* can just be `Release`... memory orderings, how do
        // they work?
        let prev = State::worker_handoff(&self.state);
        debug_assert!(prev.is_pushed());

        if prev.is_running() {
            // Wakeup the backup thread
            self.park.notify();
            false
        } else {
            true
        }
    }

    /// Terminate the worker
    pub fn signal_stop(&self) {
        let prev: State = self.state.fetch_xor(TERMINATED | PUSHED, AcqRel).into();

        debug_assert!(!prev.is_terminated());
        debug_assert!(prev.is_pushed());

        if prev.is_running() {
            self.park.notify();
        }
    }

    /// Release the worker
    pub fn release(&self) {
        let prev: State = self.state.fetch_xor(RUNNING, AcqRel).into();

        debug_assert!(prev.is_running());
    }

    /// Wait for a worker handoff
    pub fn wait_for_handoff(&self, timeout: Option<Duration>) -> Handoff {
        let sleep_until = timeout.map(|dur| Instant::now() + dur);
        let mut state: State = self.state.load(Acquire).into();

        // Run in a loop since there can be spurious wakeups
        loop {
            if !state.is_pushed() {
                if state.is_terminated() {
                    return Handoff::Terminated;
                }

                let worker_id = unsafe { (*self.handoff.get()).take().expect("no worker handoff") };
                return Handoff::Worker(worker_id);
            }

            match sleep_until {
                None => {
                    self.park.park_sync(None);
                    state = self.state.load(Acquire).into();
                }
                Some(when) => {
                    let now = Instant::now();

                    if now < when {
                        self.park.park_sync(Some(when - now));
                        state = self.state.load(Acquire).into();
                    } else {
                        debug_assert!(state.is_running());

                        // Transition out of running
                        let mut next = state;
                        next.unset_running();

                        let actual = self
                            .state
                            .compare_and_swap(state.into(), next.into(), AcqRel)
                            .into();

                        if actual == state {
                            debug_assert!(!next.is_running());
                            return Handoff::Idle;
                        }

                        state = actual;
                    }
                }
            }
        }
    }

    pub fn is_pushed(&self) -> bool {
        let state: State = self.state.load(Relaxed).into();
        state.is_pushed()
    }

    pub fn set_pushed(&self, ordering: Ordering) {
        let prev: State = self.state.fetch_or(PUSHED, ordering).into();
        debug_assert!(!prev.is_pushed());
    }

    #[inline]
    pub fn next_sleeper(&self) -> BackupId {
        unsafe { *self.next_sleeper.get() }
    }

    #[inline]
    pub fn set_next_sleeper(&self, val: BackupId) {
        unsafe {
            *self.next_sleeper.get() = val;
        }
    }
}

// ===== impl State =====

impl State {
    /// Returns a new, default, thread `State`
    pub fn new() -> State {
        State(0)
    }

    /// Returns true if the thread entry is pushed in the sleeper stack
    pub fn is_pushed(&self) -> bool {
        self.0 & PUSHED == PUSHED
    }

    fn unset_pushed(&mut self) {
        self.0 &= !PUSHED;
    }

    pub fn is_running(&self) -> bool {
        self.0 & RUNNING == RUNNING
    }

    pub fn set_running(&mut self) {
        self.0 |= RUNNING;
    }

    pub fn unset_running(&mut self) {
        self.0 &= !RUNNING;
    }

    pub fn is_terminated(&self) -> bool {
        self.0 & TERMINATED == TERMINATED
    }

    fn worker_handoff(state: &AtomicUsize) -> State {
        let mut curr: State = state.load(Acquire).into();

        loop {
            let mut next = curr;
            next.set_running();
            next.unset_pushed();

            let actual = state
                .compare_and_swap(curr.into(), next.into(), AcqRel)
                .into();

            if actual == curr {
                return curr;
            }

            curr = actual;
        }
    }
}

impl From<usize> for State {
    fn from(src: usize) -> State {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> usize {
        src.0
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("backup::State")
            .field("is_pushed", &self.is_pushed())
            .field("is_running", &self.is_running())
            .field("is_terminated", &self.is_terminated())
            .finish()
    }
}
