use task::CanBlock;

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

/// State tracking task level state to support `blocking`.
///
/// This tracks two separate flags.
///
/// a) If the task is queued in the pending blocking channel. This prevents
///    double queuing (which would break the linked list).
///
/// b) If the task has been allocated capacity to block.
#[derive(Eq, PartialEq)]
pub(crate) struct BlockingState(usize);

const QUEUED: usize = 0b01;
const ALLOCATED: usize = 0b10;

impl BlockingState {
    /// Create a new, default, `BlockingState`.
    pub fn new() -> BlockingState {
        BlockingState(0)
    }

    /// Returns `true` if the state represents the associated task being queued
    /// in the pending blocking capacity channel
    pub fn is_queued(&self) -> bool {
        self.0 & QUEUED == QUEUED
    }

    /// Toggle the queued flag
    ///
    /// Returns the state before the flag has been toggled.
    pub fn toggle_queued(state: &AtomicUsize, ordering: Ordering) -> BlockingState {
        state.fetch_xor(QUEUED, ordering).into()
    }

    /// Returns `true` if the state represents the associated task having been
    /// allocated capacity to block.
    pub fn is_allocated(&self) -> bool {
        self.0 & ALLOCATED == ALLOCATED
    }

    /// Atomically consume the capacity allocation and return if the allocation
    /// was present.
    ///
    /// If this returns `true`, then the task has the ability to block for the
    /// duration of the `poll`.
    pub fn consume_allocation(state: &AtomicUsize, ordering: Ordering) -> CanBlock {
        let state: Self = state.fetch_and(!ALLOCATED, ordering).into();

        if state.is_allocated() {
            CanBlock::Allocated
        } else if state.is_queued() {
            CanBlock::NoCapacity
        } else {
            CanBlock::CanRequest
        }
    }

    pub fn notify_blocking(state: &AtomicUsize, ordering: Ordering) {
        let prev: Self = state.fetch_xor(ALLOCATED | QUEUED, ordering).into();

        debug_assert!(prev.is_queued());
        debug_assert!(!prev.is_allocated());
    }
}

impl From<usize> for BlockingState {
    fn from(src: usize) -> BlockingState {
        BlockingState(src)
    }
}

impl From<BlockingState> for usize {
    fn from(src: BlockingState) -> usize {
        src.0
    }
}

impl fmt::Debug for BlockingState {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BlockingState")
            .field("is_queued", &self.is_queued())
            .field("is_allocated", &self.is_allocated())
            .finish()
    }
}
