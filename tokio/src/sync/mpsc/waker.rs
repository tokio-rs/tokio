use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{fence, AtomicUsize};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::{RawWaker, RawWakerVTable, Waker};

const EMPTY: usize = 0;
const REGISTERED: usize = 1;
const WAKING: usize = 2;

// `Waker::noop` requires 1.85
fn noop_waker() -> Waker {
    const NOOP_VTABLE: &RawWakerVTable = &RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), NOOP_VTABLE),
        |_| (),
        |_| (),
        |_| (),
    );
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), NOOP_VTABLE)) }
}

/// An atomic waker with relaxed synchronization.
///
/// `SpmcWaker` relies on an external synchronization which must ensure that a previous `register`
/// is visible in `wake`. This is achieved using RMWs for the wake condition access with
/// appropriate orderings.
///
/// `SpmcWaker` keep the latest registered waker in cache to save most of the reference counting
/// operations.
///
/// Algorithm adapted from [`SpmcWaker<Unsynchronized>`](https://crates.io/crates/spmc-waker).
/// The original crate couldn't be used because it requires an MSRV greater or equal to 1.91.
#[derive(Debug)]
pub(crate) struct SpmcWaker {
    state: AtomicUsize,
    waker: UnsafeCell<Waker>,
}

impl SpmcWaker {
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicUsize::new(EMPTY),
            waker: UnsafeCell::new(noop_waker()),
        }
    }

    /// Tries registering a waker, fails if there is a concurrent `wake`.
    ///
    /// # Safety
    ///
    /// `try_register` and `register` must not be called concurrently from multiple threads
    pub(crate) unsafe fn try_register(&self, waker: &Waker) -> bool {
        // Acquire is not needed to load the state. In fact, if the waker is already cached,
        // the cell is not touched. And if the previous operation was a register, it must have
        // happened on the same thread as per function safety condition.
        let state = self.state.load(Relaxed);
        // Check to use the fast path: register a waker which was previously cached. In that case
        // the state is simply updated.
        // SAFETY: as per function safety contract, there can't be concurrent mutable access
        // to the cell
        if state == EMPTY && self.waker.with(|w| unsafe { (*w).will_wake(waker) }) {
            self.state.store(REGISTERED, Release);
            true
        } else {
            self.try_register_cold(waker, state)
        }
    }

    #[cold]
    fn try_register_cold(&self, waker: &Waker, state: usize) -> bool {
        // If there is a concurrent `wake`, the wake condition is surely already met,
        // so we can return false. However, if the wake condition is not met, the
        // task must be rescheduled, which is equivalent to spinning. Tools like loom
        // requires to insert a `hint::spin_loop` to break the spin loop.
        if state == WAKING {
            return false;
        }
        if state == REGISTERED {
            // If the waker is already registered, there is nothing else to do.
            // SAFETY: as per function safety contract, there can't be concurrent
            // mutable access to the cell
            if self.waker.with(|w| unsafe { (*w).will_wake(waker) }) {
                return true;
            }
            // Otherwise, the waker must be unregistered before replacing it.
            // Updating the waker cell requires Acquire ordering to synchronize with
            // the release store of a previous `wake`.
            if let Err(s) = self.state.compare_exchange(REGISTERED, 0, Acquire, Relaxed) {
                // A concurrent `wake` happened, return as above.
                debug_assert!(s == WAKING || s == EMPTY);
                return false;
            }
        } else {
            // Updating the waker cell requires an Acquire fence to synchronize with
            // the release store of a previous `wake`.
            fence(Acquire);
        }
        // Replaces the waker and update the state using Release ordering, so the waker
        // can be accessed after an Acquire CAS in `wake`.
        // SAFETY: The state is EMPTY, so a concurrent `wake` cannot access to the cell as
        // its CAS would fail
        let _cached_waker = self.waker.with_mut(|w| unsafe { w.replace(waker.clone()) });
        self.state.store(REGISTERED, Release);
        true
    }

    /// Unregisters a previously registered waker.
    ///
    /// # Safety
    ///
    /// `try_register` and `register` must not be called concurrently from multiple threads
    pub(crate) unsafe fn unregister(&self) -> bool {
        self.state.load(Relaxed) == REGISTERED
            && (self.state)
                .compare_exchange(REGISTERED, EMPTY, Relaxed, Relaxed)
                .is_ok()
    }

    /// Unregisters a previously registered waker if any, and remove it from the cache.
    ///
    /// This function is best-effort, it may fail silently if there is a concurrent `wake`.
    ///
    /// # Safety
    ///
    /// This function calls `try_register` and assumes the same safety contract.
    pub(crate) unsafe fn reset(&self) {
        // SAFETY: as per function safety contract
        unsafe { self.try_register(&noop_waker()) };
    }

    /// Wake the registered waker.
    pub(crate) fn wake(&self) {
        // `SpmcWaker` relies on external synchronization to ensure Relaxed ordering
        // can be used here and still see the waker previously registered.
        if self.state.load(Relaxed) == REGISTERED {
            self.wake_cold();
        }
    }

    // In some workflows, like a MPSC channel with a non-empty buffer,
    // the common case is to not have a waker registered. Outlining the
    // wake path gives thus a better inlining.
    #[cold]
    fn wake_cold(&self) {
        // Tries to acquire the registered waker, using Acquire ordering to synchronize
        // with the Release store in `try_register`.
        // There is no need to retry in case of failure, as it means that a concurrent `wake`
        // has already done the job, or that the waker was unregistered.
        if (self.state)
            .compare_exchange(REGISTERED, WAKING, Acquire, Relaxed)
            .is_ok()
        {
            // The state must be reset **after** the call to `wake_by_ref`,
            // to ensure the waker reference stays alive during the call.
            // However, if `wake_by_ref` panics, the state should also be
            // reset, so we use a Drop guard for this purpose.
            struct ResetState<'a>(&'a SpmcWaker);
            impl Drop for ResetState<'_> {
                fn drop(&mut self) {
                    self.0.state.store(EMPTY, Release);
                }
            }
            //
            let _guard = ResetState(self);
            // SAFETY: the waker cell cannot be modified while the state is WAKING,
            // so it safe to access it by const reference.
            self.waker.with(|w| unsafe { (*w).wake_by_ref() })
        }
    }
}
