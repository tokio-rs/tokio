//! Source of time abstraction.
//!
//! By default, `std::time::Instant::now()` is used. However, when the
//! `test-util` feature flag is enabled, the values returned for `now()` are
//! configurable.

cfg_not_test_util! {
    use crate::time::{Duration, Instant};

    #[derive(Debug, Clone)]
    pub(crate) struct Clock {}

    pub(crate) fn now() -> Instant {
        Instant::from_std(std::time::Instant::now())
    }

    impl Clock {
        pub(crate) fn new() -> Clock {
            Clock {}
        }

        pub(crate) fn now(&self) -> Instant {
            now()
        }

        pub(crate) fn is_frozen(&self) -> bool {
            false
        }

        pub(crate) fn advance(&self, _dur: Duration) {
            unreachable!();
        }
    }
}

cfg_test_util! {
    use crate::time::{Duration, Instant};
    use std::sync::{Arc, Mutex};
    use crate::runtime::context;

    /// A handle to a source of time.
    #[derive(Debug, Clone)]
    pub(crate) struct Clock {
        inner: Arc<Inner>,
    }

    #[derive(Debug)]
    struct Inner {
        /// Instant at which the clock was created. Also incremented by the
        /// Duration that the clock was advanced by while paused.
        start: Mutex<std::time::Instant>,

        /// Instant at which the last resume was called
        since_started: Mutex<std::time::Instant>,

        /// Current, "frozen" time as an offset from `start`.
        frozen: Mutex<Option<Duration>>,
    }

    /// Pause time
    ///
    /// Freezes time. The current value of `Instant::now()` is saved and all subsequent calls
    /// to `Instant::now()` will return the saved value. This is useful for
    /// running tests that are dependent on time. To alter time for tests use
    /// [`advance()`] and [`resume()`] in conjunction with `pause()`
    ///
    /// [`advance()`]: fn.advance.html
    /// [`resume()`]: fn.resume.html
    ///
    /// # Panics
    ///
    /// Panics if time is already frozen or if called from outside of the Tokio
    /// runtime.
    pub fn pause() {
        let clock = context::clock().expect("time cannot be frozen from outside the Tokio runtime");
        //to prevent deadlock, lock frozen first, always
        let mut frozen = clock.inner.frozen.lock().unwrap();
        if frozen.is_some() {
            panic!("time is already frozen");
        }
        *frozen = Some(clock.inner.since_started.lock().unwrap().elapsed());
    }

    /// Resume time
    ///
    /// Restarts the underlying clock after it was frozen by [`pause()`]. It readjusts
    /// the value returned by `Instant::now()` such that a call to it will never yield an
    /// `Instant` that is prior to the one returned before the `resume()`. To alter time for tests use
    /// [`pause()`] and [`advance()`] in conjunction with `resume()`
    ///
    /// [`advance()`]: fn.advance.html
    /// [`pause()`]: fn.pause.html
    ///
    /// # Panics
    ///
    /// Panics if time is not frozen or if called from outside of the Tokio
    /// runtime.
    pub fn resume() {
        let clock = context::clock().expect("time cannot be frozen from outside the Tokio runtime");

        //to prevent deadlock, lock frozen first, always
        if let Some(frozen) = clock.inner.frozen.lock().unwrap().take() {
            let mut start = clock.inner.start.lock().unwrap();
            *clock.inner.since_started.lock().unwrap() = std::time::Instant::now();
            *start += frozen;
        } else {
            panic!("time is not frozen");
        };
    }

    /// Advance time
    ///
    /// Increments the saved `Instant::now()` value by `duration`. Subsequent
    /// calls to `Instant::now()` will return the result of the increment. To alter time for tests use
    /// [`pause()`] and [`resume()`] in conjunction with `advance()`
    ///
    /// [`resume()`]: fn.resume.html
    /// [`pause()`]: fn.pause.html
    ///
    /// # Panics
    ///
    /// Panics if time is not frozen or if called from outside of the Tokio
    /// runtime.
    pub async fn advance(duration: Duration) {
        let clock = context::clock().expect("time cannot be frozen from outside the Tokio runtime");
        clock.advance(duration);
        crate::task::yield_now().await;
    }

    /// Return the current instant, factoring in frozen time.
    pub(crate) fn now() -> Instant {
        if let Some(clock) = context::clock() {
            //to prevent deadlock, lock frozen first, always
            if let Some(frozen) = *clock.inner.frozen.lock().unwrap() {
                let start = clock.inner.start.lock().unwrap();
                Instant::from_std(*start + frozen)
            } else {
                //to prevent deadlock, lock start, then since_started
                let start = clock.inner.start.lock().unwrap();
                let since_started = clock.inner.since_started.lock().unwrap();
                Instant::from_std(*start + since_started.elapsed())
            }
        } else {
            Instant::from_std(std::time::Instant::now())
        }
    }

    impl Clock {
        /// Return a new `Clock` instance that uses the current execution context's
        /// source of time.
        pub(crate) fn new() -> Clock {
            Clock {
                inner: Arc::new(Inner {
                    start: Mutex::new(std::time::Instant::now()),
                    since_started: Mutex::new(std::time::Instant::now()),
                    frozen: Mutex::new(None),
                }),
            }
        }

        // TODO: delete this. Some tests rely on this
        #[cfg(all(test, not(loom)))]
        /// Return a new `Clock` instance that uses the current execution context's
        /// source of time.
        pub(crate) fn new_frozen() -> Clock {
            Clock {
                inner: Arc::new(Inner {
                    start: Mutex::new(std::time::Instant::now()),
                    since_started: Mutex::new(std::time::Instant::now()),
                    frozen: Mutex::new(Some(Duration::from_millis(0))),
                }),
            }
        }

        pub(crate) fn is_frozen(&self) -> bool {
            self.inner.frozen.lock().unwrap().is_some()
        }

        pub(crate) fn advance(&self, duration: Duration) {
            let mut frozen = self.inner.frozen.lock().unwrap();

            if let Some(ref mut elapsed) = *frozen {
                *elapsed += duration;
            } else {
                panic!("time is not frozen");
            }
        }

        // TODO: delete this as well
        #[cfg(all(test, not(loom)))]
        pub(crate) fn advanced(&self) -> Duration {
            self.inner.frozen.lock().unwrap().unwrap()
        }

        pub(crate) fn now(&self) -> Instant {
            //to prevent deadlock, lock frozen first, always
            Instant::from_std(if let Some(frozen) = *self.inner.frozen.lock().unwrap() {
                let start = self.inner.start.lock().unwrap();
                *start + frozen
            } else {
                //to prevent deadlock, lock start, then since_started
                let start = self.inner.start.lock().unwrap();
                let since_started = self.inner.since_started.lock().unwrap();
                *start + since_started.elapsed()
            })
        }
    }
}
