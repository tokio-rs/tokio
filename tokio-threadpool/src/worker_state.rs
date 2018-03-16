use std::fmt;

/// Tracks worker state
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct WorkerState(usize);

// Some constants used to work with State
// const A: usize: 0;

// TODO: This should be split up between what is accessed by each thread and
// what is concurrent. The bits accessed by each thread should be sized to
// exactly one cache line.

/// Set when the worker is pushed onto the scheduler's stack of sleeping
/// threads.
pub(crate) const PUSHED_MASK: usize = 0b001;

/// Manages the worker lifecycle part of the state
const WORKER_LIFECYCLE_MASK: usize = 0b1110;
const WORKER_LIFECYCLE_SHIFT: usize = 1;

/// The worker does not currently have an associated thread.
pub(crate) const WORKER_SHUTDOWN: usize = 0;

/// The worker is currently processing its task.
pub(crate) const WORKER_RUNNING: usize = 1;

/// The worker is currently asleep in the condvar
pub(crate) const WORKER_SLEEPING: usize = 2;

/// The worker has been notified it should process more work.
pub(crate) const WORKER_NOTIFIED: usize = 3;

/// A stronger form of notification. In this case, the worker is expected to
/// wakeup and try to acquire more work... if it enters this state while already
/// busy with other work, it is expected to signal another worker.
pub(crate) const WORKER_SIGNALED: usize = 4;

impl WorkerState {
    /// Returns true if the worker entry is pushed in the sleeper stack
    pub fn is_pushed(&self) -> bool {
        self.0 & PUSHED_MASK == PUSHED_MASK
    }

    pub fn set_pushed(&mut self) {
        self.0 |= PUSHED_MASK
    }

    pub fn is_notified(&self) -> bool {
        match self.lifecycle() {
            WORKER_NOTIFIED | WORKER_SIGNALED => true,
            _ => false,
        }
    }

    pub fn lifecycle(&self) -> usize {
        (self.0 & WORKER_LIFECYCLE_MASK) >> WORKER_LIFECYCLE_SHIFT
    }

    pub fn set_lifecycle(&mut self, val: usize) {
        self.0 = (self.0 & !WORKER_LIFECYCLE_MASK) |
            (val << WORKER_LIFECYCLE_SHIFT)
    }

    pub fn is_signaled(&self) -> bool {
        self.lifecycle() == WORKER_SIGNALED
    }

    pub fn notify(&mut self) {
        if self.lifecycle() != WORKER_SIGNALED {
            self.set_lifecycle(WORKER_NOTIFIED)
        }
    }
}

impl Default for WorkerState {
    fn default() -> WorkerState {
        // All workers will start pushed in the sleeping stack
        WorkerState(PUSHED_MASK)
    }
}

impl From<usize> for WorkerState {
    fn from(src: usize) -> Self {
        WorkerState(src)
    }
}

impl From<WorkerState> for usize {
    fn from(src: WorkerState) -> Self {
        src.0
    }
}

impl fmt::Debug for WorkerState {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("WorkerState")
            .field("lifecycle", &match self.lifecycle() {
                WORKER_SHUTDOWN => "WORKER_SHUTDOWN",
                WORKER_RUNNING => "WORKER_RUNNING",
                WORKER_SLEEPING => "WORKER_SLEEPING",
                WORKER_NOTIFIED => "WORKER_NOTIFIED",
                WORKER_SIGNALED => "WORKER_SIGNALED",
                _ => unreachable!(),
            })
            .field("is_pushed", &self.is_pushed())
            .finish()
    }
}
