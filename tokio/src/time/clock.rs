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
        pub(crate) fn new() -> Clock {
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
    use crate::time::{Duration, Instant, SystemTime};
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
        /// Instant to use as the clock's base instant.
        base: LocalInstant,

        /// Instant at which the clock was last unfrozen
        unfrozen: Option<LocalInstant>,
    }

    #[derive(Debug, Copy, Clone)]
    struct LocalInstant {
        instant: std::time::Instant,
        system: std::time::SystemTime,
    }

    #[derive(Debug, Copy, Clone)]
    struct LocalDuration {
        instant: Duration,
        system: Duration,
    }

    impl LocalInstant {
        fn now() -> Self {
            Self {
                instant: std::time::Instant::now(),
                system: std::time::SystemTime::now(),
            }
        }

        fn elapsed(&self) -> LocalDuration {
            LocalDuration {
                instant: self.instant.elapsed(),
                system: self.system.elapsed().expect("Time went backwards!"),
            }
        }
    }

    impl std::ops::AddAssign<LocalDuration> for LocalInstant {
        fn add_assign(&mut self, other: LocalDuration) {
            self.instant += other.instant;
            self.system += other.system;
        }
    }

    impl std::ops::AddAssign<Duration> for LocalInstant {
        fn add_assign(&mut self, other: Duration) {
            self.instant += other;
            self.system += other;
        }
    }

    /// Pause time
    ///
    /// The current value of `Instant::now()` is saved and all subsequent calls
    /// to `Instant::now()` until the timer wheel is checked again will return the saved value.
    /// Once the timer wheel is checked, time will immediately advance to the next registered
    /// `Sleep`. This is useful for running tests that depend on time.
    ///
    /// # Panics
    ///
    /// Panics if time is already frozen or if called from outside of the Tokio
    /// runtime.
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

        inner.unfrozen = Some(LocalInstant::now());
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
        use crate::future::poll_fn;
        use std::task::Poll;

        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        clock.advance(duration);

        let mut yielded = false;
        poll_fn(|cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }).await;
    }

    /// Return the current instant, factoring in frozen time.
    pub(crate) fn instant_now() -> Instant {
        if let Some(clock) = clock() {
            clock.instant_now()
        } else {
            Instant::from_std(std::time::Instant::now())
        }
    }

    pub(crate) fn system_now() -> SystemTime {
        if let Some(clock) = clock() {
            clock.system_now()
        } else {
            SystemTime::from_std(std::time::SystemTime::now())
        }
    }

    impl Clock {
        /// Return a new `Clock` instance that uses the current execution context's
        /// source of time.
        pub(crate) fn new() -> Clock {
            let now = LocalInstant::now();

            Clock {
                inner: Arc::new(Mutex::new(Inner {
                    base: now,
                    unfrozen: Some(now),
                })),
            }
        }

        pub(crate) fn pause(&self) {
            let mut inner = self.inner.lock().unwrap();

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

        pub(crate) fn instant_now(&self) -> Instant {
            let inner = self.inner.lock().unwrap();

            let mut ret = inner.base;

            if let Some(unfrozen) = inner.unfrozen {
                ret += unfrozen.elapsed();
            }

            Instant::from_std(ret.instant)
        }

        pub(crate) fn system_now(&self) -> SystemTime {
            let inner = self.inner.lock().unwrap();

            let mut ret = inner.base;

            if let Some(unfrozen) = inner.unfrozen {
                ret += unfrozen.elapsed();
            }

            SystemTime::from_std(ret.system)
        }
    }
}
