use config::MAX_WORKERS;
use worker;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed};
use std::{fmt, usize};

/// Lock-free stack of sleeping workers.
///
/// This is implemented as a Treiber stack and references to nodes are
/// `usize` values, indexing the entry in the `[worker::Entry]` array stored by
/// `Pool`. Each `Entry` instance maintains a `pushed` bit in its state. This
/// bit tracks if the entry is already pushed onto the stack or not. A single
/// entry can only be stored on the stack a single time.
///
/// By using indexes instead of pointers, that allows a much greater amount of
/// data to be used for the ABA guard (see correctness section of wikipedia
/// page).
///
/// Treiber stack: https://en.wikipedia.org/wiki/Treiber_Stack
#[derive(Debug)]
pub(crate) struct Stack {
    state: AtomicUsize,
}

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
pub struct State(usize);

/// Extracts the head of the worker stack from the scheduler state
///
/// The 16 relates to the value of MAX_WORKERS
const STACK_MASK: usize = ((1 << 16) - 1);

/// Used to mark the stack as empty
pub(crate) const EMPTY: usize = MAX_WORKERS;

/// Used to mark the stack as terminated
pub(crate) const TERMINATED: usize = EMPTY + 1;

/// How many bits the Treiber ABA guard is offset by
const ABA_GUARD_SHIFT: usize = 16;

#[cfg(target_pointer_width = "64")]
const ABA_GUARD_MASK: usize = (1 << (64 - ABA_GUARD_SHIFT)) - 1;

#[cfg(target_pointer_width = "32")]
const ABA_GUARD_MASK: usize = (1 << (32 - ABA_GUARD_SHIFT)) - 1;

// ===== impl Stack =====

impl Stack {
    /// Create a new `Stack` representing the empty state.
    pub fn new() -> Stack {
        let state = AtomicUsize::new(State::new().into());
        Stack { state }
    }

    /// Push a worker onto the stack
    ///
    /// # Return
    ///
    /// Returns `Ok` on success.
    ///
    /// Returns `Err` if the pool has transitioned to the `TERMINATED` state.
    /// When terminated, pushing new entries is no longer permitted.
    pub fn push(&self, entries: &[worker::Entry], idx: usize) -> Result<(), ()> {
        let mut state: State = self.state.load(Acquire).into();

        debug_assert!(worker::State::from(entries[idx].state.load(Relaxed)).is_pushed());

        loop {
            let mut next = state;

            let head = state.head();

            if head == TERMINATED {
                // The pool is terminated, cannot push the sleeper.
                return Err(());
            }

            entries[idx].set_next_sleeper(head);
            next.set_head(idx);

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if state == actual {
                return Ok(());
            }

            state = actual;
        }
    }

    /// Pop a worker off the stack.
    ///
    /// If `terminate` is set and the stack is empty when this function is
    /// called, the state of the stack is transitioned to "terminated". At this
    /// point, no further workers can be pushed onto the stack.
    ///
    /// # Return
    ///
    /// Returns the index of the popped worker and the worker's observed state.
    ///
    /// `None` if the stack is empty.
    pub fn pop(
        &self,
        entries: &[worker::Entry],
        max_lifecycle: worker::Lifecycle,
        terminate: bool,
    ) -> Option<(usize, worker::State)> {
        // Figure out the empty value
        let terminal = match terminate {
            true => TERMINATED,
            false => EMPTY,
        };

        // If terminating, the max lifecycle *must* be `Signaled`, which is the
        // highest lifecycle. By passing the greatest possible lifecycle value,
        // no entries are skipped by this function.
        //
        // TODO: It would be better to terminate in a separate function that
        // atomically takes all values and transitions to a terminated state.
        debug_assert!(!terminate || max_lifecycle == worker::Lifecycle::Signaled);

        let mut state: State = self.state.load(Acquire).into();

        loop {
            let head = state.head();

            if head == EMPTY {
                let mut next = state;
                next.set_head(terminal);

                if next == state {
                    debug_assert!(terminal == EMPTY);
                    return None;
                }

                let actual = self
                    .state
                    .compare_and_swap(state.into(), next.into(), AcqRel)
                    .into();

                if actual != state {
                    state = actual;
                    continue;
                }

                return None;
            } else if head == TERMINATED {
                return None;
            }

            debug_assert!(head < MAX_WORKERS);

            let mut next = state;

            let next_head = entries[head].next_sleeper();

            // TERMINATED can never be set as the "next pointer" on a worker.
            debug_assert!(next_head != TERMINATED);

            if next_head == EMPTY {
                next.set_head(terminal);
            } else {
                next.set_head(next_head);
            }

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if actual == state {
                // Release ordering is needed to ensure that unsetting the
                // `pushed` flag happens after popping the sleeper from the
                // stack.
                //
                // Acquire ordering is required to acquire any memory associated
                // with transitioning the worker's lifecycle.
                let state = entries[head].fetch_unset_pushed(AcqRel);

                if state.lifecycle() >= max_lifecycle {
                    // If the worker has already been notified, then it is
                    // warming up to do more work. In this case, try to pop
                    // another thread that might be in a relaxed state.
                    continue;
                }

                return Some((head, state));
            }

            state = actual;
        }
    }
}

// ===== impl State =====

impl State {
    #[inline]
    fn new() -> State {
        State(EMPTY)
    }

    #[inline]
    fn head(&self) -> usize {
        self.0 & STACK_MASK
    }

    #[inline]
    fn set_head(&mut self, val: usize) {
        // The ABA guard protects against the ABA problem w/ Treiber stacks
        let aba_guard = ((self.0 >> ABA_GUARD_SHIFT) + 1) & ABA_GUARD_MASK;

        self.0 = (aba_guard << ABA_GUARD_SHIFT) | val;
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
        let head = self.head();

        let mut fmt = fmt.debug_struct("stack::State");

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
