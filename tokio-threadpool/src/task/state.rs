#[repr(usize)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum State {
    /// Task is currently idle
    Idle = 0,

    /// Task is currently running
    Running = 1,

    /// Task is currently running, but has been notified that it must run again.
    Notified = 2,

    /// Task has been scheduled
    Scheduled = 3,

    /// Task is complete
    Complete = 4,

    /// Task was aborted because the thread pool has been shut down
    Aborted = 5,
}

// ===== impl State =====

impl State {
    /// Returns the initial task state.
    ///
    /// Tasks start in the scheduled state as they are immediately scheduled on
    /// creation.
    pub fn new() -> State {
        State::Scheduled
    }

    pub fn stub() -> State {
        State::Idle
    }
}

impl From<usize> for State {
    fn from(src: usize) -> Self {
        use self::State::*;

        debug_assert!(
            src >= Idle as usize && src <= Aborted as usize,
            "actual={}",
            src
        );

        unsafe { ::std::mem::transmute(src) }
    }
}

impl From<State> for usize {
    fn from(src: State) -> Self {
        src as usize
    }
}
