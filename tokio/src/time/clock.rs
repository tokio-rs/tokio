#![cfg_attr(not(feature = "rt"), allow(dead_code))]

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
        pub(crate) fn new(_enable_pausing: bool, _start_paused: bool) -> Clock {
            Clock {}
        }

        pub(crate) fn now(&self) -> Instant {
            now()
        }

        pub(crate) fn is_paused(&self) -> bool {
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

    cfg_rt! {
        fn clock() -> Option<Clock> {
            crate::runtime::context::clock()
        }
    }

    cfg_not_rt! {
        fn clock() -> Option<Clock> {
            None
        }
    }

    /// A handle to a source of time.
    #[derive(Debug, Clone)]
    pub(crate) struct Clock {
        inner: Arc<Mutex<Inner>>,
    }

    #[derive(Debug)]
    struct Inner {
        /// True if the ability to pause time is enabled.
        enable_pausing: bool,

        /// Instant to use as the clock's base instant.
        base: std::time::Instant,

        /// Instant at which the clock was last unfrozen
        unfrozen: Option<std::time::Instant>,
    }

    /// Pause time
    ///
    /// The current value of `Instant::now()` is saved and all subsequent calls
    /// to `Instant::now()` until the timer wheel is checked again will return
    /// the saved value. Once the timer wheel is checked, time will immediately
    /// advance to the next registered `Sleep`. This is useful for running tests
    /// that depend on time.
    ///
    /// Pausing time requires the `current_thread` Tokio runtime. This is the
    /// default runtime used by `#[tokio::test]`. The runtime can be initialized
    /// with time in a paused state using the `Builder::start_paused` method.
    ///
    /// # Panics
    ///
    /// Panics if time is already frozen or if called from outside of a
    /// `current_thread` Tokio runtime.
    pub fn pause() {
        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        clock.pause();
    }

    /// Resume time
    ///
    /// Clears the saved `Instant::now()` value. Subsequent calls to
    /// `Instant::now()` will return the value returned by the system call.
    ///
    /// # Panics
    ///
    /// Panics if time is not frozen or if called from outside of the Tokio
    /// runtime.
    pub fn resume() {
        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        let mut inner = clock.inner.lock().unwrap();

        if inner.unfrozen.is_some() {
            panic!("time is not frozen");
        }

        inner.unfrozen = Some(std::time::Instant::now());
    }

    /// Advance time
    ///
    /// Increments the saved `Instant::now()` value by `duration`. Subsequent
    /// calls to `Instant::now()` will return the result of the increment.
    ///
    /// # Panics
    ///
    /// Panics if time is not frozen or if called from outside of the Tokio
    /// runtime.
    pub async fn advance(duration: Duration) {
        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        let until = clock.now() + duration;
        clock.advance(duration);

        crate::time::sleep_until(until).await;
    }

    /// Return the current instant, factoring in frozen time.
    pub(crate) fn now() -> Instant {
        if let Some(clock) = clock() {
            clock.now()
        } else {
            Instant::from_std(std::time::Instant::now())
        }
    }

    impl Clock {
        /// Return a new `Clock` instance that uses the current execution context's
        /// source of time.
        pub(crate) fn new(enable_pausing: bool, start_paused: bool) -> Clock {
            let now = std::time::Instant::now();

            let clock = Clock {
                inner: Arc::new(Mutex::new(Inner {
                    enable_pausing,
                    base: now,
                    unfrozen: Some(now),
                })),
            };

            if start_paused {
                clock.pause();
            }

            clock
        }

        pub(crate) fn pause(&self) {
            let mut inner = self.inner.lock().unwrap();

            if !inner.enable_pausing {
                drop(inner); // avoid poisoning the lock
                panic!("`time::pause()` requires the `current_thread` Tokio runtime. \
                        This is the default Runtime used by `#[tokio::test].");
            }

            let elapsed = inner.unfrozen.as_ref().expect("time is already frozen").elapsed();
            inner.base += elapsed;
            inner.unfrozen = None;
        }

        pub(crate) fn is_paused(&self) -> bool {
            let inner = self.inner.lock().unwrap();
            inner.unfrozen.is_none()
        }

        pub(crate) fn advance(&self, duration: Duration) {
            let mut inner = self.inner.lock().unwrap();

            if inner.unfrozen.is_some() {
                panic!("time is not frozen");
            }

            inner.base += duration;
        }

        pub(crate) fn now(&self) -> Instant {
            let inner = self.inner.lock().unwrap();

            let mut ret = inner.base;

            if let Some(unfrozen) = inner.unfrozen {
                ret += unfrozen.elapsed();
            }

            Instant::from_std(ret)
        }
    }
}
