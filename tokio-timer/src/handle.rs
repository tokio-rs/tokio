use {Error, Sleep};
use timer::{Registration, Inner};

use tokio_executor::Enter;

use std::cell::RefCell;
use std::sync::{Arc, Weak};
use std::time::Instant;

/// Handle to the timer
#[derive(Debug, Clone)]
pub struct Handle {
    inner: Weak<Inner>,
}

/// Tracks the timer for the current execution context.
thread_local!(static CURRENT_TIMER: RefCell<Option<Handle>> = RefCell::new(None));

/// Set the default timer for the duration of the closure
///
/// # Panics
///
/// This function panics if there already is a default timer set.
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
    pub fn current() -> Handle {
        Handle::try_current()
            .unwrap_or(Handle { inner: Weak::new() })
    }

    /// Create a `Sleep` driven by this handle's associated `Timer`.
    pub fn sleep(&self, deadline: Instant) -> Result<Sleep, Error> {
        let registration = Registration::new_with_handle(deadline, self.clone())?;
        Ok(Sleep::new_with_registration(deadline, registration))
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
