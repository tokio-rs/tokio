use crate::runtime::time::EntryTransitionToWakingUp;
use crate::runtime::time::{TimeSource, WakeQueue, Wheel};
use std::fmt;

cfg_test_util! {
    use crate::loom::sync::Arc;
    use crate::loom::sync::atomic::{AtomicBool, Ordering};
}

/// Handle to time driver instance.
pub(crate) struct Handle {
    pub(super) time_source: TimeSource,

    // When `true`, a call to `park_timeout` should immediately return and time
    // should not advance. One reason for this to be `true` is if the task
    // passed to `Runtime::block_on` called `task::yield_now()`.
    //
    // While it may look racy, it only has any effect when the clock is paused
    // and pausing the clock is restricted to a single-threaded runtime.
    #[cfg(feature = "test-util")]
    pub(super) did_wake: Arc<AtomicBool>,
}

impl Handle {
    pub(crate) fn process_at_time(
        &self,
        wheel: &mut Wheel,
        mut now: u64,
        wake_queue: &mut WakeQueue,
    ) {
        if now < wheel.elapsed() {
            // Time went backwards! This normally shouldn't happen as the Rust language
            // guarantees that an Instant is monotonic, but can happen when running
            // Linux in a VM on a Windows host due to std incorrectly trusting the
            // hardware clock to be monotonic.
            //
            // See <https://github.com/tokio-rs/tokio/issues/3619> for more information.
            now = wheel.elapsed();
        }

        while let Some(hdl) = wheel.poll(now) {
            match hdl.transition_to_waking_up() {
                EntryTransitionToWakingUp::Success => {
                    // Safety:
                    //
                    // 1. this entry is not in the timer wheel
                    // 2. AND this entry is not in any cancellation queue
                    unsafe {
                        wake_queue.push_front(hdl);
                    }
                }
                EntryTransitionToWakingUp::Cancelling => {
                    // cancellation happens concurrently, no need to wake
                }
            }
        }
    }

    pub(crate) fn shutdown(&self, wheel: &mut Wheel) {
        // self.is_shutdown.store(true, Ordering::SeqCst);
        // Advance time forward to the end of time.
        // This will ensure that all timers are fired.
        let max_tick = u64::MAX;
        let mut wake_queue = WakeQueue::new();
        self.process_at_time(wheel, max_tick, &mut wake_queue);
        wake_queue.wake_all();
    }

    /// Returns the time source associated with this handle.
    pub(crate) fn time_source(&self) -> &TimeSource {
        &self.time_source
    }

    /// Track that the driver is being unparked
    pub(crate) fn unpark(&self) {
        #[cfg(feature = "test-util")]
        self.did_wake.store(true, Ordering::SeqCst);
    }

    cfg_test_util! {
        pub(crate) fn did_wake(&self) -> bool {
            self.did_wake.swap(false, Ordering::SeqCst)
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
        /// It can be triggered when [`Builder::enable_time`] or
        /// [`Builder::enable_all`] are not included in the builder.
        ///
        /// It can also panic whenever a timer is created outside of a
        /// Tokio runtime. That is why `rt.block_on(sleep(...))` will panic,
        /// since the function is executed outside of the runtime.
        /// Whereas `rt.block_on(async {sleep(...).await})` doesn't panic.
        /// And this is because wrapping the function on an async makes it lazy,
        /// and so gets executed inside the runtime successfully without
        /// panicking.
        ///
        /// [`Builder::enable_time`]: crate::runtime::Builder::enable_time
        /// [`Builder::enable_all`]: crate::runtime::Builder::enable_all
        #[track_caller]
        pub(crate) fn current() -> Self {
            panic!("{}", crate::util::error::CONTEXT_MISSING_ERROR)
        }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}
