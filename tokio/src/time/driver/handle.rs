use crate::time::driver::Inner;
use crate::time::Error;

use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, Weak};

/// Handle to time driver instance.
#[derive(Debug, Clone)]
pub(crate) struct Handle {
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
pub(crate) struct DefaultGuard<'a> {
    prev: Option<HandlePriv>,
    _lifetime: PhantomData<&'a u8>,
}

impl Drop for DefaultGuard<'_> {
    fn drop(&mut self) {
        CURRENT_TIMER.with(|current| {
            let mut current = current.borrow_mut();
            *current = self.prev.take();
        })
    }
}

///Sets handle to default timer, returning guard that unsets it on drop.
///
/// # Panics
///
/// This function panics if there already is a default timer set.
pub(crate) fn set_default(handle: &Handle) -> DefaultGuard<'_> {
    CURRENT_TIMER.with(|current| {
        let mut current = current.borrow_mut();
        let prev = current.take();

        let handle = handle
            .as_priv()
            .unwrap_or_else(|| panic!("`handle` does not reference a timer"));

        *current = Some(handle.clone());

        DefaultGuard {
            prev,
            _lifetime: PhantomData,
        }
    })
}

impl Handle {
    pub(crate) fn new(inner: Weak<Inner>) -> Handle {
        let inner = HandlePriv { inner };
        Handle { inner: Some(inner) }
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HandlePriv")
    }
}
