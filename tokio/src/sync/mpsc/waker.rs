use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::AtomicUsize;
use std::mem;
use std::mem::MaybeUninit;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::Waker;

const EMPTY: usize = 0;
const REGISTERED: usize = 1;
const WAKING: usize = 2;

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
    waker: UnsafeCell<MaybeUninit<Waker>>,
}

impl SpmcWaker {
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicUsize::new(EMPTY),
            waker: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Tries registering a waker, fails if there is a concurrent `wake`.
    ///
    /// # Safety
    ///
    /// `try_register` and `unregister` must not be called concurrently from multiple threads
    pub(crate) unsafe fn try_register(&self, waker: &Waker) -> bool {
        // Load the state with Acquire ordering to synchronize the cell access
        // with the Release store in `wake`.
        let state = self.state.load(Acquire);
        // There should be no waker registered, otherwise jump to the cold path.
        if state != EMPTY {
            return self.try_register_cold(waker, state);
        }
        // Replaces the waker and update the state using Release ordering, so the waker
        // can be accessed after an Acquire CAS in `wake`.
        // SAFETY: state is EMPTY, so `wake` cannot access the cell (and `try_register`
        // safety contract prevents other concurrent accesses)
        (self.waker).with_mut(|w| unsafe { (*w).write(waker.clone()) });
        self.state.store(REGISTERED, Release);
        true
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
        debug_assert_eq!(state, REGISTERED);
        // A waker is registered, it must be unregistered before replacing it.
        // In case of concurrent `wake`, the CAS would fail and the waker cell
        // would not be modified, so Acquire ordering is not needed.
        if let Err(s) = (self.state).compare_exchange(REGISTERED, EMPTY, Relaxed, Relaxed) {
            // A concurrent `wake` happened, return as above.
            debug_assert!(s == WAKING || s == EMPTY);
            return false;
        }
        // Set back the state to REGISTERED using Release ordering, so the waker
        // can be accessed after an Acquire CAS in `wake`.
        // If `Waker::clone` panics, the guard will still set the state, which
        // is correct as the previous waker is still stored.
        struct RegisterGuard<'a>(&'a SpmcWaker);
        impl Drop for RegisterGuard<'_> {
            fn drop(&mut self) {
                self.0.state.store(REGISTERED, Release);
            }
        }
        let mut _old_waker = None; // declared before RegisterGuard so drop is executed after
        let _register_guard = RegisterGuard(self);
        _old_waker = self.waker.with_mut(|old_waker| {
            // SAFETY: state is EMPTY, so `wake` cannot access the cell (and `try_register`
            // safety contract prevents other concurrent accesses)
            // SAFETY: the previously registered waker is still stored in the cell
            let old_waker = unsafe { (*old_waker).assume_init_mut() };
            // Replace the waker only if it is different from the one already stored.
            (!old_waker.will_wake(waker)).then(|| mem::replace(old_waker, waker.clone()))
        });
        true
    }

    /// Unregisters a previously registered waker.
    ///
    /// # Safety
    ///
    /// `try_register` and `unregister` must not be called concurrently from multiple threads
    #[inline]
    pub(crate) unsafe fn unregister(&self) {
        // In case of concurrent `wake`, the CAS would fail and the waker cell
        // would not be modified, so Acquire ordering is not needed.
        if self.state.load(Relaxed) == REGISTERED
            && (self.state)
                .compare_exchange(REGISTERED, EMPTY, Relaxed, Relaxed)
                .is_ok()
        {
            // SAFETY: The state is EMPTY, so a concurrent `wake` cannot access to the cell as
            // its CAS would fail
            self.waker.with_mut(|w| unsafe { (*w).assume_init_drop() });
        }
    }

    /// Wake the registered waker.
    #[inline]
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
            // SAFETY: the waker cell cannot be modified while the state is WAKING,
            // so it safe to access it immutably.
            let waker = self.waker.with(|w| unsafe { (*w).assume_init_read() });
            // Reset the state with Release ordering to synchronize the cell access
            // with the Acquire load in `try_register`.
            self.state.store(EMPTY, Release);
            waker.wake();
        }
    }
}

impl Drop for SpmcWaker {
    fn drop(&mut self) {
        let state = self.state.with_mut(|s| *s);
        debug_assert!(state == EMPTY || state == REGISTERED);
        if state == REGISTERED {
            // SAFETY: a waker is registered
            self.waker.with_mut(|w| unsafe { (*w).assume_init_drop() })
        }
    }
}
