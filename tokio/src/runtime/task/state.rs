use crate::loom::sync::atomic::AtomicUsize;

use std::fmt;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::usize;

pub(super) struct State {
    val: AtomicUsize,
}

/// Current state value
#[derive(Copy, Clone, Eq, PartialEq)]
pub(super) struct Snapshot(usize);

type UpdateResult = Result<Snapshot, Snapshot>;

// The bits described below are also documented in src/runtime/task/mod.rs

/// The task is currently not being run.
///
/// Unused when COMPLETE is set, so it may take any value for completed tasks.
const IDLE: usize = 0b0001;

/// The task is complete.
///
/// Once this bit is set, it is never unset
const COMPLETE: usize = 0b0010;

/// Flag tracking if a Notified currently exists for this task.
const NOTIFIED: usize = 0b0100;

/// Flag tracking if a notification is sent while a task is being polled. May
/// take any value if not currently being polled.
///
/// This is necessary because when a task is polled, the Notified still exists.
const NOTIFIED_DURING_POLL: usize = 0b1000;

/// The join handle is still around
const JOIN_INTEREST: usize = 0b1_0000;

/// A join handle waker has been set
const JOIN_WAKER: usize = 0b10_0000;

/// The task has been forcibly cancelled.
const CANCELLED: usize = 0b100_0000;

/// All bits
const STATE_MASK: usize =
    IDLE | COMPLETE | NOTIFIED | NOTIFIED_DURING_POLL | JOIN_INTEREST | JOIN_WAKER | CANCELLED;

/// Bits used by the ref count portion of the state.
const REF_COUNT_MASK: usize = !STATE_MASK;

/// Number of positions to shift the ref count
const REF_COUNT_SHIFT: usize = REF_COUNT_MASK.count_zeros() as usize;

/// One ref count
const REF_ONE: usize = 1 << REF_COUNT_SHIFT;

/// State a task is initialized with
///
/// A task is initialized with three references:
///
///  * A OwnedTask reference that will be stored in an OwnedTasks or LocalOwnedTasks.
///  * A Notifed reference that will be sent to the scheduler as a notification.
///  * A JoinHandle reference.
///
/// However it is only initialized with two ref-counts as the Notified does not
/// hold a ref-count.
///
/// As the task starts with a `JoinHandle`, `JOIN_INTEREST` is set.
/// As the task starts with a `Notified`, `NOTIFIED` is set.
const INITIAL_STATE: usize = (REF_ONE * 2) | JOIN_INTEREST | NOTIFIED | IDLE;

pub(super) enum IdleTransition {
    Ok,
    OkNotified,
    Cancelled,
}

/// All transitions are performed via RMW operations. This establishes an
/// unambiguous modification order.
impl State {
    /// Return a task's initial state
    pub(super) fn new() -> State {
        State {
            val: AtomicUsize::new(INITIAL_STATE),
        }
    }

    /// Loads the current state, establishes `Acquire` ordering.
    pub(super) fn load(&self) -> Snapshot {
        Snapshot(self.val.load(Acquire))
    }

    /// Attempt to transition the lifecycle to `Running`.
    ///
    /// Returns an error if the task is already running or completed. The return
    /// value contains a snapshot of the state **after** the transition.
    pub(super) fn transition_to_running(&self) -> UpdateResult {
        // unset IDLE and NOTIFIED_DURING_POLL
        const MASK: usize = !(IDLE | NOTIFIED_DURING_POLL);

        let prev = Snapshot(self.val.fetch_and(MASK, AcqRel));
        let next = Snapshot(prev.0 & MASK);

        if prev.is_idle_runnable() {
            Ok(next)
        } else {
            Err(next)
        }
    }

    /// Transitions the task from `Running` -> `Idle`.
    ///
    /// This task assumes that the caller has ownership of a Notified and will
    /// destroy it unless NOTIFIED_DURING_POLL is set.
    ///
    /// If the task is cancelled, the transition fails and the caller should
    /// cancel the task. The NOTIFIED bit is not unset in this case.
    pub(super) fn transition_to_idle(&self) -> IdleTransition {
        let mut prev = Snapshot(0);
        let _ = self.fetch_update(|mut snapshot| {
            prev = snapshot;

            if snapshot.is_cancelled() {
                return None;
            }

            snapshot.set_idle();

            // Conceptually the transition to idle always consumes the NOTIFIED
            // bit that the caller owned, but if we were notified during poll,
            // a new notification is immediately created and submitted to the
            // runtime, hence we do not unset the bit in that case.
            if !snapshot.is_notified_during_poll() {
                snapshot.unset_notified();
            }

            Some(snapshot)
        });

        if prev.is_cancelled() {
            IdleTransition::Cancelled
        } else if prev.is_notified_during_poll() {
            IdleTransition::OkNotified
        } else {
            IdleTransition::Ok
        }
    }

    /// Transitions the task from `Running` -> `Complete`.
    pub(super) fn transition_to_complete(&self) -> Snapshot {
        // set COMPLETE
        let prev = Snapshot(self.val.fetch_or(COMPLETE, AcqRel));
        let next = Snapshot(prev.0 | COMPLETE);

        #[cfg(any(test, debug_assertions))]
        assert!(prev.is_running() && !prev.is_complete());

        next
    }

    /// Transitions the state to `NOTIFIED`.
    ///
    /// Returns `true` if this operation obtained ownership of a Notified that
    /// should be submitted to the pool for execution.
    pub(super) fn transition_to_notified(&self) -> bool {
        const MASK: usize = NOTIFIED | NOTIFIED_DURING_POLL;

        let prev = Snapshot(self.val.fetch_or(MASK, AcqRel));

        // We do not care whether the task is completed. The NOTIFIED bit acts
        // as a ref-count, and we must create a Notified object or otherwise
        // unset the bit again to not leak the task.
        !prev.is_notified()
    }

    /// Set the cancelled bit and transition the state to `NOTIFIED`.
    ///
    /// Returns `true` if this operation obtained ownership of a Notified that
    /// should be submitted to the pool for execution.
    pub(super) fn transition_to_notified_and_cancel(&self) -> bool {
        const MASK: usize = CANCELLED | NOTIFIED | NOTIFIED_DURING_POLL;

        let prev = Snapshot(self.val.fetch_or(MASK, AcqRel));

        // We do not care whether the task is completed. The NOTIFIED bit acts
        // as a ref-count, and we must create a Notified object or otherwise
        // unset the bit again to not leak the task.
        !prev.is_notified()
    }

    /// Set the `CANCELLED` bit and attempt to transition to `Running`.
    ///
    /// Returns `true` if the transition to `Running` succeeded. If the
    /// transition fails, the provided closure can be used to decrement a
    /// ref-count.
    pub(super) fn transition_to_shutdown(&self) -> bool {
        let mut prev = Snapshot(0);

        let _ = self.fetch_update(|mut snapshot| {
            prev = snapshot;

            if snapshot.is_idle_runnable() {
                snapshot.unset_idle();
            }

            snapshot.set_cancelled();
            Some(snapshot)
        });

        prev.is_idle_runnable()
    }

    /// Optimistically tries to swap the state assuming the join handle is
    /// __immediately__ dropped on spawn
    pub(super) fn drop_join_handle_fast(&self) -> Result<(), ()> {
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
                INITIAL_STATE,
                (INITIAL_STATE - REF_ONE) & !JOIN_INTEREST,
                Release,
                Relaxed,
            )
            .map(|_| ())
            .map_err(|_| ())
    }

    /// Try to unset the JOIN_INTEREST flag.
    ///
    /// Returns `Ok` if the operation happens before the task transitions to a
    /// completed state, `Err` otherwise.
    pub(super) fn unset_join_interested(&self) -> UpdateResult {
        self.fetch_update(|curr| {
            assert!(curr.is_join_interested());

            if curr.is_complete() {
                return None;
            }

            let mut next = curr;
            next.unset_join_interested();

            Some(next)
        })
    }

    /// Set the `JOIN_WAKER` bit.
    ///
    /// Returns `Ok` if the bit is set, `Err` otherwise. This operation fails if
    /// the task has completed.
    pub(super) fn set_join_waker(&self) -> UpdateResult {
        self.fetch_update(|curr| {
            assert!(curr.is_join_interested());
            assert!(!curr.has_join_waker());

            if curr.is_complete() {
                return None;
            }

            let mut next = curr;
            next.set_join_waker();

            Some(next)
        })
    }

    /// Unsets the `JOIN_WAKER` bit.
    ///
    /// Returns `Ok` has been unset, `Err` otherwise. This operation fails if
    /// the task has completed.
    pub(super) fn unset_waker(&self) -> UpdateResult {
        self.fetch_update(|curr| {
            assert!(curr.is_join_interested());
            assert!(curr.has_join_waker());

            if curr.is_complete() {
                return None;
            }

            let mut next = curr;
            next.unset_join_waker();

            Some(next)
        })
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
        let prev = self.val.fetch_add(REF_ONE, Relaxed);

        // If the reference count overflowed, abort.
        if prev > isize::MAX as usize {
            process::abort();
        }
    }

    /// Returns `true` if the task should be released.
    pub(super) fn ref_dec(&self) -> bool {
        let prev = Snapshot(self.val.fetch_sub(REF_ONE, AcqRel));
        let next = Snapshot(prev.0 - REF_ONE);
        next.should_dealloc()
    }

    /// Returns `true` if the task should be released.
    pub(super) fn ref_dec_several(&self, count: usize) -> bool {
        let prev = Snapshot(self.val.fetch_sub(count * REF_ONE, AcqRel));
        let next = Snapshot(prev.0 - count * REF_ONE);
        next.should_dealloc()
    }

    /// Unset NOTIFIED and decrement the ref-count the specific number of times.
    ///
    /// Returns `true` if the task should be released.
    pub(super) fn unset_notified(&self, ref_dec: usize) -> bool {
        if ref_dec > 0 {
            let diff = NOTIFIED | (ref_dec * REF_ONE);

            let prev = Snapshot(self.val.fetch_sub(diff, AcqRel));
            let next = Snapshot(prev.0.wrapping_sub(diff));

            #[cfg(any(test, debug_assertions))]
            assert!(prev.is_notified());

            #[cfg(any(test, debug_assertions))]
            assert!(prev.ref_count() >= ref_dec);

            next.should_dealloc()
        } else {
            let prev = Snapshot(self.val.fetch_and(!NOTIFIED, AcqRel));
            let next = Snapshot(prev.0 & !NOTIFIED);

            #[cfg(any(test, debug_assertions))]
            assert!(prev.is_notified());

            next.should_dealloc()
        }
    }

    fn fetch_update<F>(&self, mut f: F) -> Result<Snapshot, Snapshot>
    where
        F: FnMut(Snapshot) -> Option<Snapshot>,
    {
        let mut curr = self.load();

        loop {
            let next = match f(curr) {
                Some(next) => next,
                None => return Err(curr),
            };

            let res = self.val.compare_exchange(curr.0, next.0, AcqRel, Acquire);

            match res {
                Ok(_) => return Ok(next),
                Err(actual) => curr = Snapshot(actual),
            }
        }
    }
}

// ===== impl Snapshot =====

impl Snapshot {
    /// Returns `true` if the task is idle and runnable.
    pub(super) fn is_idle_runnable(self) -> bool {
        self.0 & (IDLE | COMPLETE) == IDLE
    }

    /// Returns `true` if the task has been flagged as notified.
    pub(super) fn is_notified(self) -> bool {
        self.0 & NOTIFIED == NOTIFIED
    }

    pub(super) fn unset_notified(&mut self) {
        self.0 &= !NOTIFIED;
    }

    /// Returns `true` if the task has been flagged as notified during a poll.
    pub(super) fn is_notified_during_poll(self) -> bool {
        self.0 & NOTIFIED_DURING_POLL == NOTIFIED_DURING_POLL
    }

    pub(super) fn is_running(self) -> bool {
        self.0 & IDLE == 0
    }

    fn set_idle(&mut self) {
        self.0 |= IDLE;
    }

    fn unset_idle(&mut self) {
        self.0 &= !IDLE;
    }

    pub(super) fn is_cancelled(self) -> bool {
        self.0 & CANCELLED == CANCELLED
    }

    fn set_cancelled(&mut self) {
        self.0 |= CANCELLED;
    }

    /// Returns `true` if the task's future has completed execution.
    pub(super) fn is_complete(self) -> bool {
        self.0 & COMPLETE == COMPLETE
    }

    pub(super) fn is_join_interested(self) -> bool {
        self.0 & JOIN_INTEREST == JOIN_INTEREST
    }

    fn unset_join_interested(&mut self) {
        self.0 &= !JOIN_INTEREST
    }

    pub(super) fn has_join_waker(self) -> bool {
        self.0 & JOIN_WAKER == JOIN_WAKER
    }

    fn set_join_waker(&mut self) {
        self.0 |= JOIN_WAKER;
    }

    fn unset_join_waker(&mut self) {
        self.0 &= !JOIN_WAKER
    }

    pub(super) fn should_dealloc(self) -> bool {
        // The Notified holds the task alive without a ref-count
        self.0 & (REF_COUNT_MASK | NOTIFIED) == 0
    }

    fn ref_count(self) -> usize {
        (self.0 & REF_COUNT_MASK) >> REF_COUNT_SHIFT
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.load();
        snapshot.fmt(fmt)
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Snapshot")
            .field("is_running", &self.is_running())
            .field("is_complete", &self.is_complete())
            .field("is_notified", &self.is_notified())
            .field("is_cancelled", &self.is_cancelled())
            .field("is_join_interested", &self.is_join_interested())
            .field("has_join_waker", &self.has_join_waker())
            .field("ref_count", &self.ref_count())
            .finish()
    }
}
