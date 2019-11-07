use timer::Inner;
use {Deadline, Delay, Error, Interval, Timeout};

use tokio_executor::Enter;

use std::cell::RefCell;
use std::fmt;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// Handle to timer instance.
///
/// The `Handle` allows creating `Delay` instances that are driven by the
/// associated timer.
///
/// A `Handle` is obtained by calling [`Timer::handle`], [`Handle::current`], or
/// [`Handle::default`].
///
/// * [`Timer::handle`]: returns a handle associated with the specific timer.
///   The handle will always reference the same timer.
///
/// * [`Handle::current`]: returns a handle to the timer for the execution
///   context **at the time the function is called**. This function must be
///   called from a runtime that has an associated timer or it will panic.
///   The handle will always reference the same timer.
///
/// * [`Handle::default`]: returns a handle to the timer for the execution
///   context **at the time the handle is used**. This function is safe to call
///   at any time. The handle may reference different specific timer instances.
///   Calling `Handle::default().delay(...)` is always equivalent to
///   `Delay::new(...)`.
///
/// [`Timer::handle`]: struct.Timer.html#method.handle
/// [`Handle::current`]: #method.current
/// [`Handle::default`]: #method.default
#[derive(Debug, Clone)]
pub struct Handle {
    inner: Option<HandlePriv>,
}

/// Like `Handle` but never `None`.
#[derive(Clone)]
pub(crate) struct HandlePriv {
    inner: Weak<Inner>,
}

/// A guard that resets the current timer to `None` when dropped.
#[derive(Debug)]
pub struct DefaultGuard {
    _p: (),
}

thread_local! {
    /// Tracks the timer for the current execution context.
    static CURRENT_TIMER: RefCell<Option<HandlePriv>> = RefCell::new(None)
}

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
where
    F: FnOnce(&mut Enter) -> R,
{
    let _guard = set_default(handle);
    f(enter)
}

/// Sets `handle` as the default timer, returning a guard that unsets it on drop.
///
/// # Panics
///
/// This function panics if there already is a default timer set.
pub fn set_default(handle: &Handle) -> DefaultGuard {
    CURRENT_TIMER.with(|current| {
        let mut current = current.borrow_mut();

        assert!(
            current.is_none(),
            "default Tokio timer already set \
             for execution context"
        );

        let handle = handle
            .as_priv()
            .unwrap_or_else(|| panic!("`handle` does not reference a timer"));

        *current = Some(handle.clone());
    });
    DefaultGuard { _p: () }
}

impl Handle {
    pub(crate) fn new(inner: Weak<Inner>) -> Handle {
        let inner = HandlePriv { inner };
        Handle { inner: Some(inner) }
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
    /// See [type] level documentation for more ways to obtain a `Handle` value.
    ///
    /// [`with_default`]: ../fn.with_default.html
    /// [type]: #
    pub fn current() -> Handle {
        let private =
            HandlePriv::try_current().unwrap_or_else(|_| HandlePriv { inner: Weak::new() });

        Handle {
            inner: Some(private),
        }
    }

    /// Create a `Delay` driven by this handle's associated `Timer`.
    pub fn delay(&self, deadline: Instant) -> Delay {
        match self.inner {
            Some(ref handle_priv) => Delay::new_with_handle(deadline, handle_priv.clone()),
            None => Delay::new(deadline),
        }
    }

    #[doc(hidden)]
    #[deprecated(since = "0.2.11", note = "use timeout instead")]
    pub fn deadline<T>(&self, future: T, deadline: Instant) -> Deadline<T> {
        Deadline::new_with_delay(future, self.delay(deadline))
    }

    /// Create a `Timeout` driven by this handle's associated `Timer`.
    pub fn timeout<T>(&self, value: T, deadline: Instant) -> Timeout<T> {
        Timeout::new_with_delay(value, self.delay(deadline))
    }

    /// Create a new `Interval` that starts at `at` and yields every `duration`
    /// interval after that.
    pub fn interval(&self, at: Instant, duration: Duration) -> Interval {
        Interval::new_with_delay(self.delay(at), duration)
    }

    fn as_priv(&self) -> Option<&HandlePriv> {
        self.inner.as_ref()
    }
}

impl Default for Handle {
    fn default() -> Handle {
        Handle { inner: None }
    }
}

impl HandlePriv {
    /// Try to get a handle to the current timer.
    ///
    /// Returns `Err` if no handle is found.
    pub(crate) fn try_current() -> Result<HandlePriv, Error> {
        CURRENT_TIMER.with(|current| match *current.borrow() {
            Some(ref handle) => Ok(handle.clone()),
            None => Err(Error::shutdown()),
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

impl fmt::Debug for HandlePriv {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "HandlePriv")
    }
}

impl Drop for DefaultGuard {
    fn drop(&mut self) {
        let _ = CURRENT_TIMER.try_with(|current| {
            let mut current = current.borrow_mut();
            *current = None;
        });
    }
}
