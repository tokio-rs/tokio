use std::{fmt, usize};

/// ThreadPool state.
///
/// The two least significant bits are the shutdown flags.  (0 for active, 1 for
/// shutdown on idle, 2 for shutting down). The remaining bits represent the
/// number of futures that still need to complete.
#[derive(Eq, PartialEq, Clone, Copy)]
pub(crate) struct State(usize);

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
#[repr(usize)]
pub(crate) enum Lifecycle {
    /// The thread pool is currently running
    Running = 0,

    /// The thread pool should shutdown once it reaches an idle state.
    ShutdownOnIdle = 1,

    /// The thread pool should start the process of shutting down.
    ShutdownNow = 2,
}

/// Mask used to extract the number of futures from the state
const LIFECYCLE_MASK: usize = 0b11;
const NUM_FUTURES_MASK: usize = !LIFECYCLE_MASK;
const NUM_FUTURES_OFFSET: usize = 2;

/// Max number of futures the pool can handle.
pub(crate) const MAX_FUTURES: usize = usize::MAX >> NUM_FUTURES_OFFSET;

// ===== impl State =====

impl State {
    #[inline]
    pub fn new() -> State {
        State(0)
    }

    /// Returns the number of futures still pending completion.
    pub fn num_futures(&self) -> usize {
        self.0 >> NUM_FUTURES_OFFSET
    }

    /// Increment the number of futures pending completion.
    ///
    /// Returns false on failure.
    pub fn inc_num_futures(&mut self) {
        debug_assert!(self.num_futures() < MAX_FUTURES);
        debug_assert!(self.lifecycle() < Lifecycle::ShutdownNow);

        self.0 += 1 << NUM_FUTURES_OFFSET;
    }

    /// Decrement the number of futures pending completion.
    pub fn dec_num_futures(&mut self) {
        let num_futures = self.num_futures();

        if num_futures == 0 {
            // Already zero
            return;
        }

        self.0 -= 1 << NUM_FUTURES_OFFSET;

        if self.lifecycle() == Lifecycle::ShutdownOnIdle && num_futures == 1 {
            self.set_lifecycle(Lifecycle::ShutdownNow);
        }
    }

    /// Set the number of futures pending completion to zero
    pub fn clear_num_futures(&mut self) {
        self.0 = self.0 & LIFECYCLE_MASK;
    }

    pub fn lifecycle(&self) -> Lifecycle {
        (self.0 & LIFECYCLE_MASK).into()
    }

    pub fn set_lifecycle(&mut self, val: Lifecycle) {
        self.0 = (self.0 & NUM_FUTURES_MASK) | (val as usize);
    }

    pub fn is_terminated(&self) -> bool {
        self.lifecycle() == Lifecycle::ShutdownNow && self.num_futures() == 0
    }
}

impl From<usize> for State {
    fn from(src: usize) -> Self {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> Self {
        src.0
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("pool::State")
            .field("lifecycle", &self.lifecycle())
            .field("num_futures", &self.num_futures())
            .finish()
    }
}

// ===== impl Lifecycle =====

impl From<usize> for Lifecycle {
    fn from(src: usize) -> Lifecycle {
        use self::Lifecycle::*;

        debug_assert!(
            src == Running as usize
                || src == ShutdownOnIdle as usize
                || src == ShutdownNow as usize
        );

        unsafe { ::std::mem::transmute(src) }
    }
}

impl From<Lifecycle> for usize {
    fn from(src: Lifecycle) -> usize {
        let v = src as usize;
        debug_assert!(v & LIFECYCLE_MASK == v);
        v
    }
}
