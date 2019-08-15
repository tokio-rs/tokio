use crate::clock::now;
use crate::timer::Inner;
use crate::{Delay, Error, /*Interval,*/ Timeout};
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
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

thread_local! {
    /// Tracks the timer for the current execution context.
    static CURRENT_TIMER: RefCell<Option<HandlePriv>> = RefCell::new(None)
}

#[derive(Debug)]
///Unsets default timer handler on drop.
pub struct DefaultGuard<'a> {
    _lifetime: PhantomData<&'a u8>,
}

impl Drop for DefaultGuard<'_> {
    fn drop(&mut self) {
        CURRENT_TIMER.with(|current| {
            let mut current = current.borrow_mut();
            *current = None;
        })
    }
}

///Sets handle to default timer, returning guard that unsets it on drop.
///
/// # Panics
///
/// This function panics if there already is a default timer set.
pub fn set_default(handle: &Handle) -> DefaultGuard<'_> {
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

    DefaultGuard {
        _lifetime: PhantomData,
    }
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
        self.delay_timeout(deadline, Duration::from_secs(0))
    }

    fn delay_timeout(&self, deadline: Instant, duration: Duration) -> Delay {
        match self.inner {
            Some(ref handle_priv) => {
                Delay::new_with_handle(deadline, Duration::from_secs(0), handle_priv.clone())
            }
            None => Delay::new_timeout(deadline, duration),
        }
    }

    /// Create a `Timeout` driven by this handle's associated `Timer`.
    pub fn timeout<T>(&self, value: T, timeout: Duration) -> Timeout<T> {
        Timeout::new_with_delay(value, self.delay_timeout(now() + timeout, timeout))
    }

    /*
    /// Create a new `Interval` that starts at `at` and yields every `duration`
    /// interval after that.
    pub fn interval(&self, at: Instant, duration: Duration) -> Interval {
        Interval::new_with_delay(self.delay(at), duration)
    }
    */

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HandlePriv")
    }
}
