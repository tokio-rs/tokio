use crate::time::driver::Inner;
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, Weak};

/// Handle to time driver instance.
#[derive(Clone)]
pub(crate) struct Handle {
    inner: Weak<Inner>,
}

thread_local! {
    /// Tracks the timer for the current execution context.
    static CURRENT_TIMER: RefCell<Option<Handle>> = RefCell::new(None)
}

#[derive(Debug)]
/// Guard that unsets the current default timer on drop.
pub(crate) struct DefaultGuard<'a> {
    prev: Option<Handle>,
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

/// Sets handle to default timer, returning guard that unsets it on drop.
///
/// # Panics
///
/// This function panics if there already is a default timer set.
pub(crate) fn set_default(handle: &Handle) -> DefaultGuard<'_> {
    CURRENT_TIMER.with(|current| {
        let mut current = current.borrow_mut();
        let prev = current.take();

        *current = Some(handle.clone());

        DefaultGuard {
            prev,
            _lifetime: PhantomData,
        }
    })
}

impl Handle {
    /// Create a new timer `Handle` from a shared `Inner` timer state.
    pub(crate) fn new(inner: Weak<Inner>) -> Self {
        Handle { inner }
    }

    /// Try to get a handle to the current timer.
    ///
    /// # Panics
    ///
    /// This function panics if there is no current timer set.
    pub(crate) fn current() -> Self {
        CURRENT_TIMER.with(|current| match *current.borrow() {
            Some(ref handle) => handle.clone(),
            None => panic!("no current timer"),
        })
    }

    /// Try to return a strong ref to the inner
    pub(crate) fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}
