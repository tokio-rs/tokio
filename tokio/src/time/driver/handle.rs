use crate::loom::sync::{Arc, Mutex};
use crate::time::driver::{ClockTime, TimeSource};
use std::fmt;

/// Handle to time driver instance.
#[derive(Clone)]
pub(super) struct InternalHandle<TS: TimeSource> {
    time_source: TS,
    inner: Arc<Mutex<super::Inner<TS>>>,
}

impl<TS: TimeSource> InternalHandle<TS> {
    /// Creates a new timer `Handle` from a shared `Inner` timer state.
    pub(crate) fn new(inner: Arc<Mutex<super::Inner<TS>>>) -> Self {
        let time_source = inner.lock().time_source.clone();
        InternalHandle { time_source, inner }
    }

    /// Returns the time source associated with this handle
    pub(super) fn time_source(&self) -> &TS {
        &self.time_source
    }

    /// Locks the driver's inner structure
    pub(super) fn lock(&self) -> crate::loom::sync::MutexGuard<'_, super::Inner<TS>> {
        self.inner.lock()
    }
}

/// "Public" handle, exposed outside of the time submodule. This avoids exposing
/// the timesource details to consumer code.
#[derive(Clone)]
pub(crate) struct Handle {
    inner: InternalHandle<super::ClockTime>,
}

impl From<InternalHandle<ClockTime>> for Handle {
    fn from(inner: InternalHandle<ClockTime>) -> Self {
        Self { inner }
    }
}

impl Handle {
    /// Obtains the inner InternalHandle
    pub(super) fn internal(&self) -> &InternalHandle<ClockTime> {
        &self.inner
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

impl<TS: TimeSource> fmt::Debug for InternalHandle<TS> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InternalHandle")
    }
}
