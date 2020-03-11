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

type UpdateResult = Result<Snapshot, Snapshot>;

/// The task is currently being run.
const RUNNING: usize = 0b0001;

/// The task is complete.
///
/// Once this bit is set, it is never unset
const COMPLETE: usize = 0b0010;

/// Extracts the task's lifecycle value from the state
const LIFECYCLE_MASK: usize = 0b11;

/// Flag tracking if the task has been pushed into a run queue.
const NOTIFIED: usize = 0b100;

/// The join handle is still around
const JOIN_INTEREST: usize = 0b1_000;

/// A join handle waker has been set
const JOIN_WAKER: usize = 0b10_000;

/// The task has been forcibly cancelled.
const CANCELLED: usize = 0b100_000;

/// All bits
const STATE_MASK: usize = LIFECYCLE_MASK | NOTIFIED | JOIN_INTEREST | JOIN_WAKER | CANCELLED;

/// Bits used by the ref count portion of the state.
const REF_COUNT_MASK: usize = !STATE_MASK;

/// Number of positions to shift the ref count
const REF_COUNT_SHIFT: usize = REF_COUNT_MASK.count_zeros() as usize;

/// One ref count
const REF_ONE: usize = 1 << REF_COUNT_SHIFT;

/// State a task is initialized with
///
/// A task is initialized with two references: one for the scheduler and one for
/// the `JoinHandle`. As the task starts with a `JoinHandle`, `JOIN_INTERST` is
/// set. A new task is immediately pushed into the run queue for execution and
/// starts with the `NOTIFIED` flag set.
const INITIAL_STATE: usize = (REF_ONE * 2) | JOIN_INTEREST | NOTIFIED;

/// All transitions are performed via RMW operations. This establishes an
/// unambiguous modification order.
impl State {
    /// Return a task's initial state
    pub(super) fn new() -> State {
        // A task is initialized with three references: one for the scheduler,
        // one for the `JoinHandle`, one for the task handle made available in
        // release. As the task starts with a `JoinHandle`, `JOIN_INTERST` is
        // set. A new task is immediately pushed into the run queue for
        // execution and starts with the `NOTIFIED` flag set.
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
    /// If `ref_inc` is set, the reference count is also incremented.
    ///
    /// The `NOTIFIED` bit is always unset.
    pub(super) fn transition_to_running(&self, ref_inc: bool) -> UpdateResult {
        self.fetch_update(|curr| {
            assert!(curr.is_notified());

            let mut next = curr;

            if !next.is_idle() {
                return None;
            }

            if ref_inc {
                next.ref_inc();
            }

            next.set_running();
            next.unset_notified();
            Some(next)
        })
    }

    /// Transitions the task from `Running` -> `Idle`.
    ///
    /// Returns `Ok` if the transition to `Idle` is successful, `Err` otherwise.
    /// In both cases, a snapshot of the state from **after** the transition is
    /// returned.
    ///
    /// The transition to `Idle` fails if the task has been flagged to be
    /// cancelled.
    pub(super) fn transition_to_idle(&self) -> UpdateResult {
        self.fetch_update(|curr| {
            assert!(curr.is_running());

            if curr.is_cancelled() {
                return None;
            }

            let mut next = curr;
            next.unset_running();

            if next.is_notified() {
                // The caller needs to schedule the task. To do this, it needs a
                // waker. The waker requires a ref count.
                next.ref_inc();
            }

            Some(next)
        })
    }

    /// Transitions the task from `Running` -> `Complete`.
    pub(super) fn transition_to_complete(&self) -> Snapshot {
        const DELTA: usize = RUNNING | COMPLETE;

        let prev = Snapshot(self.val.fetch_xor(DELTA, AcqRel));
        assert!(prev.is_running());
        assert!(!prev.is_complete());

        Snapshot(prev.0 ^ DELTA)
    }

    /// Transition from `Complete` -> `Terminal`, decrementing the reference
    /// count by 1.
    ///
    /// When `ref_dec` is set, an additional ref count decrement is performed.
    /// This is used to batch atomic ops when possible.
    pub(super) fn transition_to_terminal(&self, complete: bool, ref_dec: bool) -> Snapshot {
        self.fetch_update(|mut snapshot| {
            if complete {
                snapshot.set_complete();
            } else {
                assert!(snapshot.is_complete());
            }

            // Decrement the primary handle
            snapshot.ref_dec();

            if ref_dec {
                // Decrement a second time
                snapshot.ref_dec();
            }

            Some(snapshot)
        })
        .unwrap()
    }

    /// Transitions the state to `NOTIFIED`.
    ///
    /// Returns `true` if the task needs to be submitted to the pool for
    /// execution
    pub(super) fn transition_to_notified(&self) -> bool {
        let prev = Snapshot(self.val.fetch_or(NOTIFIED, AcqRel));
        prev.will_need_queueing()
    }

    /// Set the `CANCELLED` bit and attempt to transition to `Running`.
    ///
    /// Returns `true` if the transition to `Running` succeeded.
    pub(super) fn transition_to_shutdown(&self) -> bool {
        let mut prev = Snapshot(0);

        let _ = self.fetch_update(|mut snapshot| {
            prev = snapshot;

            if snapshot.is_idle() {
                snapshot.set_running();

                if snapshot.is_notified() {
                    // If the task is idle and notified, this indicates the task is
                    // in the run queue and is considered owned by the scheduler.
                    // The shutdown operation claims ownership of the task, which
                    // means we need to assign an additional ref-count to the task
                    // in the queue.
                    snapshot.ref_inc();
                }
            }

            snapshot.set_cancelled();
            Some(snapshot)
        });

        prev.is_idle()
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
        if prev > isize::max_value() as usize {
            process::abort();
        }
    }

    /// Returns `true` if the task should be released.
    pub(super) fn ref_dec(&self) -> bool {
        let prev = Snapshot(self.val.fetch_sub(REF_ONE, AcqRel));
        prev.ref_count() == 1
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
    /// Returns `true` if the task is in an idle state.
    pub(super) fn is_idle(self) -> bool {
        self.0 & (RUNNING | COMPLETE) == 0
    }

    /// Returns `true` if the task has been flagged as notified.
    pub(super) fn is_notified(self) -> bool {
        self.0 & NOTIFIED == NOTIFIED
    }

    fn unset_notified(&mut self) {
        self.0 &= !NOTIFIED
    }

    pub(super) fn is_running(self) -> bool {
        self.0 & RUNNING == RUNNING
    }

    fn set_running(&mut self) {
        self.0 |= RUNNING;
    }

    fn unset_running(&mut self) {
        self.0 &= !RUNNING;
    }

    pub(super) fn is_cancelled(self) -> bool {
        self.0 & CANCELLED == CANCELLED
    }

    fn set_cancelled(&mut self) {
        self.0 |= CANCELLED;
    }

    fn set_complete(&mut self) {
        self.0 |= COMPLETE;
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

    pub(super) fn ref_count(self) -> usize {
        (self.0 & REF_COUNT_MASK) >> REF_COUNT_SHIFT
    }

    fn ref_inc(&mut self) {
        assert!(self.0 <= isize::max_value() as usize);
        self.0 += REF_ONE;
    }

    pub(super) fn ref_dec(&mut self) {
        assert!(self.ref_count() > 0);
        self.0 -= REF_ONE
    }

    fn will_need_queueing(self) -> bool {
        !self.is_notified() && self.is_idle()
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
