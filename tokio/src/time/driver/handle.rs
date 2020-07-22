use crate::runtime::context;
use crate::time::driver::Inner;
use std::fmt;
use std::sync::{Arc, Weak};

/// Handle to time driver instance.
#[derive(Clone)]
pub(crate) struct Handle {
    inner: Weak<Inner>,
}

impl Handle {
    /// Creates a new timer `Handle` from a shared `Inner` timer state.
    pub(crate) fn new(inner: Weak<Inner>) -> Self {
        Handle { inner }
    }

    /// Tries to get a handle to the current timer.
    ///
    /// # Panics
    ///
    /// This function panics if there is no current timer set.
    pub(crate) fn current() -> Self {
        context::time_handle()
            .expect("there is no timer running, must be called from the context of Tokio runtime")
    }

    /// Tries to return a strong ref to the inner
    pub(crate) fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}
