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

    /// Tries to return a strong ref to the inner
    pub(crate) fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

cfg_rt! {
    impl Handle {
        /// Tries to get a handle to the current timer.
        ///
        /// # Panics
        ///
        /// This function panics if there is no current timer set.
        ///
        /// It can be triggered when `Builder::enable_time()` or
        /// `Builder::enable_all()` are not included in the builder.
        ///
        /// It can also panic whenever a timer is created outside of a Tokio
        /// runtime. That is why `rt.block_on(delay_for(...))` will panic,
        /// since the function is executed outside of the runtime.
        /// Whereas `rt.block_on(async {delay_for(...).await})` doesn't
        /// panic. And this is because wrapping the function on an async makes it
        /// lazy, and so gets executed inside the runtime successfuly without
        /// panicking.
        pub(crate) fn current() -> Self {
            crate::runtime::context::time_handle()
                .expect("there is no timer running, must be called from the context of Tokio runtime")
        }
    }
}

cfg_not_rt! {
    impl Handle {
        /// Tries to get a handle to the current timer.
        ///
        /// # Panics
        ///
        /// This function panics if there is no current timer set.
        ///
        /// It can be triggered when `Builder::enable_time()` or
        /// `Builder::enable_all()` are not included in the builder.
        ///
        /// It can also panic whenever a timer is created outside of a Tokio
        /// runtime. That is why `rt.block_on(delay_for(...))` will panic,
        /// since the function is executed outside of the runtime.
        /// Whereas `rt.block_on(async {delay_for(...).await})` doesn't
        /// panic. And this is because wrapping the function on an async makes it
        /// lazy, and so gets executed inside the runtime successfuly without
        /// panicking.
        pub(crate) fn current() -> Self {
            panic!("there is no timer running, must be called from the context of Tokio runtime or \
            `rt` is not enabled")
        }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}
