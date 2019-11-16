use crate::loom::sync::atomic::AtomicUsize;

use std::fmt;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::usize;

pub(super) struct State {
    val: AtomicUsize,
}

/// Current state value
#[derive(Copy, Clone)]
pub(super) struct Snapshot(usize);

/// The task is currently being run.
const RUNNING: usize = 0b00_0001;

/// The task has been notified by a waker.
const NOTIFIED: usize = 0b00_0010;

/// The task is complete.
///
/// Once this bit is set, it is never unset
const COMPLETE: usize = 0b00_0100;

/// The primary task handle has been dropped.
const RELEASED: usize = 0b00_1000;

/// The join handle is still around
const JOIN_INTEREST: usize = 0b01_0000;

/// A join handle waker has been set
const JOIN_WAKER: usize = 0b10_0000;

/// The task has been forcibly canceled.
const CANCELLED: usize = 0b100_0000;

/// All bits
const LIFECYCLE_MASK: usize =
    RUNNING | NOTIFIED | COMPLETE | RELEASED | JOIN_INTEREST | JOIN_WAKER | CANCELLED;

/// Bits used by the waker ref count portion of the state.
///
/// Ref counts only cover **wakers**. Other handles are tracked with other state
/// bits.
const WAKER_COUNT_MASK: usize = usize::MAX - LIFECYCLE_MASK;

/// Number of positions to shift the ref count
const WAKER_COUNT_SHIFT: usize = WAKER_COUNT_MASK.count_zeros() as usize;

/// One ref count
const WAKER_ONE: usize = 1 << WAKER_COUNT_SHIFT;

/// Initial state
const INITIAL_STATE: usize = NOTIFIED;

/// All transitions are performed via RMW operations. This establishes an
/// unambiguous modification order.
impl State {
    /// Starts with a ref count of 1
    #[cfg(feature = "rt-full")]
    pub(super) fn new_background() -> State {
        State {
            val: AtomicUsize::new(INITIAL_STATE),
        }
    }

    /// Starts with a ref count of 2
    pub(super) fn new_joinable() -> State {
        State {
            val: AtomicUsize::new(INITIAL_STATE | JOIN_INTEREST),
        }
    }

    /// Load the current state, establishes `Acquire` ordering.
    pub(super) fn load(&self) -> Snapshot {
        Snapshot(self.val.load(Acquire))
    }

    /// Transition a task to the `Running` state.
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn transition_to_running(&self) -> Snapshot {
        const DELTA: usize = RUNNING | NOTIFIED;

        let prev = Snapshot(self.val.fetch_xor(DELTA, Acquire));
        debug_assert!(prev.is_notified());

        if prev.is_running() {
            // We were signalled to cancel
            //
            // Apply the state
            let prev = self.val.fetch_or(CANCELLED, AcqRel);
            return Snapshot(prev | CANCELLED);
        }

        debug_assert!(!prev.is_running());

        let next = Snapshot(prev.0 ^ DELTA);

        debug_assert!(next.is_running());
        debug_assert!(!next.is_notified());

        next
    }

    /// Transition the task from `Running` -> `Idle`.
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn transition_to_idle(&self) -> Snapshot {
        const DELTA: usize = RUNNING;

        let prev = Snapshot(self.val.fetch_xor(DELTA, AcqRel));

        if !prev.is_running() {
            // We were signaled to cancel.
            //
            // Apply the state
            let prev = self.val.fetch_or(CANCELLED, AcqRel);
            return Snapshot(prev | CANCELLED);
        }

        let next = Snapshot(prev.0 ^ DELTA);

        debug_assert!(!next.is_running());

        next
    }

    /// Transition the task from `Running` -> `Complete`.
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn transition_to_complete(&self) -> Snapshot {
        const DELTA: usize = RUNNING | COMPLETE;

        let prev = Snapshot(self.val.fetch_xor(DELTA, AcqRel));

        debug_assert!(!prev.is_complete());

        let next = Snapshot(prev.0 ^ DELTA);

        debug_assert!(next.is_complete());

        next
    }

    /// Transition the task from `Running` -> `Released`.
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn transition_to_released(&self) -> Snapshot {
        const DELTA: usize = RUNNING | COMPLETE | RELEASED;

        let prev = Snapshot(self.val.fetch_xor(DELTA, AcqRel));

        debug_assert!(prev.is_running());
        debug_assert!(!prev.is_complete());
        debug_assert!(!prev.is_released());

        let next = Snapshot(prev.0 ^ DELTA);

        debug_assert!(!next.is_running());
        debug_assert!(next.is_complete());
        debug_assert!(next.is_released());

        next
    }

    /// Transition the task to the canceled state.
    ///
    /// Returns the snapshot of the state **after** the transition **if** the
    /// transition was made successfully
    ///
    /// # States
    ///
    /// - Notifed: task may be in a queue, caller must not release.
    /// - Running: cannot drop. The poll handle will handle releasing.
    /// - Other prior states do not require cancellation.
    ///
    /// If the task has been notified, then it may still be in a queue. The
    /// caller must not release the task.
    pub(super) fn transition_to_canceled_from_queue(&self) -> Snapshot {
        let prev = Snapshot(self.val.fetch_or(CANCELLED, AcqRel));

        debug_assert!(!prev.is_complete());
        debug_assert!(!prev.is_running() || prev.is_notified());

        Snapshot(prev.0 | CANCELLED)
    }

    pub(super) fn transition_to_canceled_from_list(&self) -> Option<Snapshot> {
        let mut prev = self.load();

        loop {
            if !prev.is_active() {
                return None;
            }

            let mut next = prev;

            // Use the running flag to signal cancellation
            if prev.is_running() {
                next.0 -= RUNNING;
                next.0 |= NOTIFIED;
            } else if prev.is_notified() {
                next.0 += RUNNING;
                next.0 |= NOTIFIED;
            } else {
                next.0 |= CANCELLED;
            }

            let res = self.val.compare_exchange(prev.0, next.0, AcqRel, Acquire);

            match res {
                Ok(_) if next.is_canceled() => return Some(next),
                Ok(_) => return None,
                Err(actual) => {
                    prev = Snapshot(actual);
                }
            }
        }
    }

    /// Final transition to `Released`. Called when primary task handle is
    /// dropped. This is roughly a "ref decrement" operation.
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn release_task(&self) -> Snapshot {
        use crate::loom::sync::atomic;

        const DELTA: usize = RELEASED;

        let prev = Snapshot(self.val.fetch_or(DELTA, Release));

        debug_assert!(!prev.is_released());
        debug_assert!(prev.is_terminal(), "state = {:?}", prev);

        let next = Snapshot(prev.0 | DELTA);

        debug_assert!(next.is_released());

        if next.is_final_ref() || (next.has_join_waker() && !next.is_join_interested()) {
            // The final reference to the task was dropped, the caller must free the
            // memory. Establish an acquire ordering.
            atomic::fence(Acquire);
        }

        next
    }

    /// Transition the state to `Scheduled`.
    ///
    /// Returns `true` if the task needs to be submitted to the pool for
    /// execution
    pub(super) fn transition_to_notified(&self) -> bool {
        const MASK: usize = RUNNING | NOTIFIED | COMPLETE | CANCELLED;

        let prev = self.val.fetch_or(NOTIFIED, Release);
        prev & MASK == 0
    }

    /// Optimistically try to swap the state assuming the join handle is
    /// __immediately__ dropped on spawn
    pub(super) fn drop_join_handle_fast(&self) -> bool {
        use std::sync::atomic::Ordering::Relaxed;

        // Relaxed is acceptable as if this function is called and succeeds,
        // then nothing has been done w/ the join handle.
        //
        // The moment the join handle is used (polled), the `JOIN_WAKER` flag is
        // set, at which point the CAS will fail.
        //
        // Given this, there is no risk if this operation is reordered.
        self.val
            .compare_exchange_weak(
                INITIAL_STATE | JOIN_INTEREST,
                INITIAL_STATE,
                Relaxed,
                Relaxed,
            )
            .is_ok()
    }

    /// The join handle has completed by reading the output
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn complete_join_handle(&self) -> Snapshot {
        use crate::loom::sync::atomic;

        const DELTA: usize = JOIN_INTEREST;

        let prev = Snapshot(self.val.fetch_sub(DELTA, Release));

        debug_assert!(prev.is_join_interested());

        let next = Snapshot(prev.0 - DELTA);

        if !next.is_final_ref() {
            return next;
        }

        atomic::fence(Acquire);

        next
    }

    /// The join handle is being dropped, this fails if the task has been
    /// completed and the output must be dropped first then
    /// `complete_join_handle` should be called.
    ///
    /// Returns a snapshot of the state **after** the transition.
    pub(super) fn drop_join_handle_slow(&self) -> Result<Snapshot, Snapshot> {
        const MASK: usize = COMPLETE | CANCELLED;

        let mut prev = self.val.load(Acquire);

        loop {
            // Once the complete bit is set, it is never unset.
            if prev & MASK != 0 {
                return Err(Snapshot(prev));
            }

            debug_assert!(prev & JOIN_INTEREST == JOIN_INTEREST);

            let next = (prev - JOIN_INTEREST) & !JOIN_WAKER;

            let res = self.val.compare_exchange(prev, next, AcqRel, Acquire);

            match res {
                Ok(_) => {
                    return Ok(Snapshot(next));
                }
                Err(actual) => {
                    prev = actual;
                }
            }
        }
    }

    /// Store the join waker.
    pub(super) fn store_join_waker(&self) -> Snapshot {
        use crate::loom::sync::atomic;

        const DELTA: usize = JOIN_WAKER;

        let prev = Snapshot(self.val.fetch_xor(DELTA, Release));

        debug_assert!(!prev.has_join_waker());

        let next = Snapshot(prev.0 ^ DELTA);

        debug_assert!(next.has_join_waker());

        if next.is_complete() {
            atomic::fence(Acquire);
        }

        next
    }

    pub(super) fn unset_waker(&self) -> Snapshot {
        const MASK: usize = COMPLETE | CANCELLED;

        let mut prev = self.val.load(Acquire);

        loop {
            // Once the `COMPLETE` bit is set, it is never unset
            if prev & MASK != 0 {
                return Snapshot(prev);
            }

            debug_assert!(Snapshot(prev).has_join_waker());

            let next = prev - JOIN_WAKER;

            let res = self.val.compare_exchange(prev, next, AcqRel, Acquire);

            match res {
                Ok(_) => return Snapshot(next),
                Err(actual) => {
                    prev = actual;
                }
            }
        }
    }

    pub(super) fn ref_inc(&self) {
        use std::process;
        use std::sync::atomic::Ordering::Relaxed;

        // Using a relaxed ordering is alright here, as knowledge of the
        // original reference prevents other threads from erroneously deleting
        // the object.
        //
        // As explained in the [Boost documentation][1], Increasing the
        // reference counter can always be done with memory_order_relaxed: New
        // references to an object can only be formed from an existing
        // reference, and passing an existing reference from one thread to
        // another must already provide any required synchronization.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        let prev = self.val.fetch_add(WAKER_ONE, Relaxed);

        // If the reference count overflowed, abort.
        if prev > isize::max_value() as usize {
            process::abort();
        }
    }

    /// Returns `true` if the task should be released.
    pub(super) fn ref_dec(&self) -> bool {
        use crate::loom::sync::atomic;

        let prev = self.val.fetch_sub(WAKER_ONE, Release);
        let next = Snapshot(prev - WAKER_ONE);

        if next.is_final_ref() {
            atomic::fence(Acquire);
        }

        next.is_final_ref()
    }
}

impl Snapshot {
    pub(super) fn is_running(self) -> bool {
        self.0 & RUNNING == RUNNING
    }

    pub(super) fn is_notified(self) -> bool {
        self.0 & NOTIFIED == NOTIFIED
    }

    pub(super) fn is_released(self) -> bool {
        self.0 & RELEASED == RELEASED
    }

    pub(super) fn is_complete(self) -> bool {
        self.0 & COMPLETE == COMPLETE
    }

    pub(super) fn is_canceled(self) -> bool {
        self.0 & CANCELLED == CANCELLED
    }

    /// Used during normal runtime.
    pub(super) fn is_active(self) -> bool {
        self.0 & (COMPLETE | CANCELLED) == 0
    }

    /// Used before dropping the task
    pub(super) fn is_terminal(self) -> bool {
        // When both the notified & running flags are set, the task was canceled
        // after being notified, before it was run.
        //
        // There is a race where:
        // - The task state transitions to notified
        // - The global queue is shutdown
        // - The waker attempts to push into the global queue and fails.
        // - The waker holds the last reference to the task, thus drops it.
        //
        // In this scenario, the cancelled bit will never get set.
        !self.is_active() || (self.is_notified() && self.is_running())
    }

    pub(super) fn is_join_interested(self) -> bool {
        self.0 & JOIN_INTEREST == JOIN_INTEREST
    }

    pub(super) fn has_join_waker(self) -> bool {
        self.0 & JOIN_WAKER == JOIN_WAKER
    }

    pub(super) fn is_final_ref(self) -> bool {
        const MASK: usize = WAKER_COUNT_MASK | RELEASED | JOIN_INTEREST;

        (self.0 & MASK) == RELEASED
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::sync::atomic::Ordering::SeqCst;

        let snapshot = Snapshot(self.val.load(SeqCst));

        fmt.debug_struct("State")
            .field("snapshot", &snapshot)
            .finish()
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Snapshot")
            .field("is_running", &self.is_running())
            .field("is_notified", &self.is_notified())
            .field("is_released", &self.is_released())
            .field("is_complete", &self.is_complete())
            .field("is_canceled", &self.is_canceled())
            .field("is_join_interested", &self.is_join_interested())
            .field("has_join_waker", &self.has_join_waker())
            .field("is_final_ref", &self.is_final_ref())
            .finish()
    }
}
