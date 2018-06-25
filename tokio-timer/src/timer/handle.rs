use {Error, Delay, Deadline, Interval};
use timer::{Registration, Inner};

use tokio_executor::Enter;

use std::cell::RefCell;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// Handle to timer instance.
///
/// The `Handle` allows creating `Delay` instances that are driven by the
/// associated timer.
///
/// A `Handle` is obtained by calling [`Timer::handle`].
///
/// [`Timer::handle`]: struct.Timer.html#method.handle
#[derive(Debug, Clone)]
pub struct Handle {
    inner: Weak<Inner>,
}

/// Tracks the timer for the current execution context.
thread_local!(static CURRENT_TIMER: RefCell<Option<Handle>> = RefCell::new(None));

/// Set the default timer for the duration of the closure.
///
/// From within the closure, [`Delay`] instances that are created via
/// [`Delay::new`] can be used.
///
/// # Panics
///
/// This function panics if there already is a default timer set.
///
/// [`Delay`]: ../struct.Delay.html
/// [`Delay::new`]: ../struct.Delay.html#method.new
pub fn with_default<F, R>(handle: &Handle, enter: &mut Enter, f: F) -> R
where F: FnOnce(&mut Enter) -> R
{
    // Ensure that the timer is removed from the thread-local context
    // when leaving the scope. This handles cases that involve panicking.
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT_TIMER.with(|current| {
                let mut current = current.borrow_mut();
                *current = None;
            });
        }
    }

    // This ensures the value for the current timer gets reset even if there is
    // a panic.
    let _r = Reset;

    CURRENT_TIMER.with(|current| {
        {
            let mut current = current.borrow_mut();
            assert!(current.is_none(), "default Tokio timer already set \
                    for execution context");
            *current = Some(handle.clone());
        }

        f(enter)
    })
}

impl Handle {
    pub(crate) fn new(inner: Weak<Inner>) -> Handle {
        Handle { inner }
    }

    /// Returns a handle to the current timer.
    ///
    /// The current timer is the timer that is currently set as default using
    /// [`with_default`].
    ///
    /// This function should only be called from within the context of
    /// [`with_default`]. Calling this function from outside of this context
    /// will return a `Handle` that does not reference a timer. `Delay`
    /// instances created with this handle will error.
    ///
    /// [`with_default`]: ../fn.with_default.html
    pub fn current() -> Handle {
        Handle::try_current()
            .unwrap_or(Handle { inner: Weak::new() })
    }

    /// Create a `Delay` driven by this handle's associated `Timer`.
    pub fn delay(&self, deadline: Instant) -> Delay {
        let registration = Registration::new_with_handle(deadline, self.clone());
        Delay::new_with_registration(deadline, registration)
    }

    /// Create a `Deadline` driven by this handle's associated `Timer`.
    pub fn deadline<T>(&self, future: T, deadline: Instant) -> Deadline<T> {
        Deadline::new_with_delay(future, self.delay(deadline))
    }

    /// Create a new `Interval` that starts at `at` and yields every `duration`
    /// interval after that.
    pub fn interval(&self, at: Instant, duration: Duration) -> Interval {
        Interval::new_with_delay(self.delay(at), duration)
    }

    /// Try to get a handle to the current timer.
    ///
    /// Returns `Err` if no handle is found.
    pub(crate) fn try_current() -> Result<Handle, Error> {
        CURRENT_TIMER.with(|current| {
            match *current.borrow() {
                Some(ref handle) => Ok(handle.clone()),
                None => Err(Error::shutdown()),
            }
        })
    }

    /// Try to return a strong ref to the inner
    pub(crate) fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }

    /// Consume the handle, returning the weak Inner ref.
    pub(crate) fn into_inner(self) -> Weak<Inner> {
        self.inner
    }
}
