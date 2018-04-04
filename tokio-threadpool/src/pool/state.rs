use std::{fmt, usize};

/// ThreadPool state.
///
/// The two least significant bits are the shutdown flags.  (0 for active, 1 for
/// shutdown on idle, 2 for shutting down). The remaining bits represent the
/// number of futures that still need to complete.
#[derive(Eq, PartialEq, Clone, Copy)]
pub(crate) struct PoolState(usize);

/// Flag used to track if the pool is running
pub(crate) const SHUTDOWN_ON_IDLE: usize = 1;
pub(crate) const SHUTDOWN_NOW: usize = 2;

/// Mask used to extract the number of futures from the state
const LIFECYCLE_MASK: usize = 0b11;
const NUM_FUTURES_MASK: usize = !LIFECYCLE_MASK;
const NUM_FUTURES_OFFSET: usize = 2;

/// Max number of futures the pool can handle.
pub(crate) const MAX_FUTURES: usize = usize::MAX >> NUM_FUTURES_OFFSET;

impl PoolState {
    #[inline]
    pub fn new() -> PoolState {
        PoolState(0)
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
        debug_assert!(self.lifecycle() < SHUTDOWN_NOW);

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

        if self.lifecycle() == SHUTDOWN_ON_IDLE && num_futures == 1 {
            self.0 = SHUTDOWN_NOW;
        }
    }

    /// Set the number of futures pending completion to zero
    pub fn clear_num_futures(&mut self) {
        self.0 = self.0 & LIFECYCLE_MASK;
    }

    pub fn lifecycle(&self) -> usize {
        self.0 & LIFECYCLE_MASK
    }

    pub fn set_lifecycle(&mut self, val: usize) {
        self.0 = (self.0 & NUM_FUTURES_MASK) | val;
    }

    pub fn is_terminated(&self) -> bool {
        self.lifecycle() == SHUTDOWN_NOW && self.num_futures() == 0
    }
}

impl From<usize> for PoolState {
    fn from(src: usize) -> Self {
        PoolState(src)
    }
}

impl From<PoolState> for usize {
    fn from(src: PoolState) -> Self {
        src.0
    }
}

impl fmt::Debug for PoolState {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("State")
            .field("lifecycle", &self.lifecycle())
            .field("num_futures", &self.num_futures())
            .finish()
    }
}
