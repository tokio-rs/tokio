use config::MAX_WORKERS;

use std::{fmt, usize};

/// State related to the stack of sleeping workers.
///
/// - Parked head     16 bits
/// - Sequence        remaining
///
/// The parked head value has a couple of special values:
///
/// - EMPTY: No sleepers
/// - TERMINATED: Don't spawn more threads
#[derive(Eq, PartialEq, Clone, Copy)]
pub(crate) struct SleepStack(usize);

/// Extracts the head of the worker stack from the scheduler state
const STACK_MASK: usize = ((1 << 16) - 1);

/// Used to mark the stack as empty
pub(crate) const EMPTY: usize = MAX_WORKERS;

/// Used to mark the stack as terminated
pub(crate) const TERMINATED: usize = EMPTY + 1;

/// How many bits the treiber ABA guard is offset by
const ABA_GUARD_SHIFT: usize = 16;

#[cfg(target_pointer_width = "64")]
const ABA_GUARD_MASK: usize = (1 << (64 - ABA_GUARD_SHIFT)) - 1;

#[cfg(target_pointer_width = "32")]
const ABA_GUARD_MASK: usize = (1 << (32 - ABA_GUARD_SHIFT)) - 1;

impl SleepStack {
    #[inline]
    pub fn new() -> SleepStack {
        SleepStack(EMPTY)
    }

    #[inline]
    pub fn head(&self) -> usize {
        self.0 & STACK_MASK
    }

    #[inline]
    pub fn set_head(&mut self, val: usize) {
        // The ABA guard protects against the ABA problem w/ treiber stacks
        let aba_guard = ((self.0 >> ABA_GUARD_SHIFT) + 1) & ABA_GUARD_MASK;

        self.0 = (aba_guard << ABA_GUARD_SHIFT) | val;
    }
}

impl From<usize> for SleepStack {
    fn from(src: usize) -> Self {
        SleepStack(src)
    }
}

impl From<SleepStack> for usize {
    fn from(src: SleepStack) -> Self {
        src.0
    }
}

impl fmt::Debug for SleepStack {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let head = self.head();

        let mut fmt = fmt.debug_struct("SleepStack");

        if head < MAX_WORKERS {
            fmt.field("head", &head);
        } else if head == EMPTY {
            fmt.field("head", &"EMPTY");
        } else if head == TERMINATED {
            fmt.field("head", &"TERMINATED");
        }

        fmt.finish()
    }
}
