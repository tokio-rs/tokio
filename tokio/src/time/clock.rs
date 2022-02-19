#![cfg_attr(not(feature = "rt"), allow(dead_code))]

//! Source of time abstraction.
//!
//! By default, `std::time::Instant::now()` is used. However, when the
//! `test-util` feature flag is enabled, the values returned for `now()` are
//! configurable.

cfg_not_test_util! {
    use crate::time::{Instant};

    #[derive(Debug, Clone)]
    pub(crate) struct Clock {}

    pub(crate) fn now() -> Instant {
        Instant::from_std(std::time::Instant::now())
    }

    impl Clock {
        pub(crate) fn new(_enable_pausing: bool) -> Clock {
            Clock {}
        }

        pub(crate) fn now(&self) -> Instant {
            now()
        }
    }
}

cfg_test_util! {
    use crate::time::{Duration, Instant};
    use crate::loom::sync::{Arc, Mutex};

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

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Frozen {
        Thawed(std::time::Instant),
        #[allow(clippy::enum_variant_names)]
        Frozen,
        NoAdvance,
    }

    impl Frozen {
        fn thawed(&self) -> Option<&std::time::Instant> {
            match self {
                Frozen::Thawed(ref i) => Some(i),
                _ => None,
            }
        }

        fn is_frozen(&self) -> bool {
            !matches!(self, Frozen::Thawed(_))
        }

        fn is_frozen_no_advance(&self) -> bool {
            matches!(self, Frozen::NoAdvance)
        }
    }

    #[derive(Debug)]
    struct Inner {
        /// True if the ability to pause time is enabled.
        enable_pausing: bool,

        /// Instant to use as the clock's base instant.
        base: std::time::Instant,

        /// Instant at which the clock was last thawed or freeze state.
        frozen: Frozen,
    }

    /// Pauses time.
    ///
    /// The current value of `Instant::now()` is saved and all subsequent calls
    /// to `Instant::now()` will return the saved value. The saved value can be
    /// changed by [`advance`] or by the time auto-advancing once the runtime
    /// has no work to do. This only affects the `Instant` type in Tokio, and
    /// the `Instant` in std continues to work as normal.
    ///
    /// Pausing time requires the `current_thread` Tokio runtime. This is the
    /// default runtime used by `#[tokio::test]`. The runtime can be initialized
    /// with time in a paused state using the `Builder::start_paused` method.
    ///
    /// For cases where time is immediately paused, it is better to pause
    /// the time using the `main` or `test` macro:
    /// ```
    /// #[tokio::main(flavor = "current_thread", start_paused = true)]
    /// async fn main() {
    ///    println!("Hello world");
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if time is already frozen or if called from outside of a
    /// `current_thread` Tokio runtime.
    ///
    /// # Auto-advance
    ///
    /// If time is paused and the runtime has no work to do, the clock is
    /// auto-advanced to the next pending timer. This means that [`Sleep`] or
    /// other timer-backed primitives can cause the runtime to advance the
    /// current time when awaited.
    ///
    /// [`Sleep`]: crate::time::Sleep
    /// [`advance`]: crate::time::advance
    pub fn pause() {
        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        clock.pause();
    }

    /// Pauses time and disabled auto advancing
    pub fn pause_no_advance() {
        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        clock.pause_no_advance();
    }

    /// Resumes time.
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
        let mut inner = clock.inner.lock();

        if !inner.frozen.is_frozen() {
            panic!("time is not frozen");
        }

        inner.frozen = Frozen::Thawed(std::time::Instant::now());
    }

    /// Advances time.
    ///
    /// Increments the saved `Instant::now()` value by `duration`. Subsequent
    /// calls to `Instant::now()` will return the result of the increment.
    ///
    /// This function will make the current time jump forward by the given
    /// duration in one jump. This means that all `sleep` calls with a deadline
    /// before the new time will immediately complete "at the same time", and
    /// the runtime is free to poll them in any order.  Additionally, this
    /// method will not wait for the `sleep` calls it advanced past to complete.
    /// If you want to do that, you should instead call [`sleep`] and rely on
    /// the runtime's auto-advance feature.
    ///
    /// Note that calls to `sleep` are not guaranteed to complete the first time
    /// they are polled after a call to `advance`. For example, this can happen
    /// if the runtime has not yet touched the timer driver after the call to
    /// `advance`. However if they don't, the runtime will poll the task again
    /// shortly.
    ///
    /// # Panics
    ///
    /// Panics if time is not frozen or if called from outside of the Tokio
    /// runtime.
    ///
    /// # Auto-advance
    ///
    /// If the time is paused and there is no work to do, the runtime advances
    /// time to the next timer. See [`pause`](pause#auto-advance) for more
    /// details.
    ///
    /// [`sleep`]: fn@crate::time::sleep
    pub async fn advance(duration: Duration) {
        let clock = clock().expect("time cannot be frozen from outside the Tokio runtime");
        clock.advance(duration);

        crate::task::yield_now().await;
    }

    /// Returns the current instant, factoring in frozen time.
    pub(crate) fn now() -> Instant {
        if let Some(clock) = clock() {
            clock.now()
        } else {
            Instant::from_std(std::time::Instant::now())
        }
    }

    impl Clock {
        /// Returns a new `Clock` instance that uses the current execution context's
        /// source of time.
        pub(crate) fn new(enable_pausing: bool) -> Clock {
            let now = std::time::Instant::now();

            Clock {
                inner: Arc::new(Mutex::new(Inner {
                    enable_pausing,
                    base: now,
                    frozen: Frozen::Thawed(now),
                })),
            }
        }

        fn freeze(&self, frozen: Frozen) {
            let mut inner = self.inner.lock();

            if !inner.enable_pausing {
                drop(inner); // avoid poisoning the lock
                panic!("`time::pause()` or `time::pause_no_advance()` requires the `current_thread` Tokio runtime. \
                        This is the default Runtime used by `#[tokio::test].");
            }

            match (&inner.frozen, &frozen) {
                (Frozen::Thawed(t), _) => {
                    let elapsed = t.elapsed();
                    inner.base += elapsed;
                },
                (a, b) if a != b => (),
                _ => panic!("time is already frozen"),
            }

            inner.frozen = frozen;
        }

        pub(crate) fn pause(&self) {
            self.freeze(Frozen::Frozen);
        }

        pub(crate) fn pause_no_advance(&self) {
            self.freeze(Frozen::NoAdvance);
        }

        pub(crate) fn is_paused(&self) -> bool {
            let inner = self.inner.lock();
            inner.frozen.is_frozen()
        }

        pub(crate) fn is_paused_no_advance(&self) -> bool {
            let inner = self.inner.lock();
            inner.frozen.is_frozen_no_advance()
        }

        pub(crate) fn advance(&self, duration: Duration) {
            let mut inner = self.inner.lock();

            if !inner.frozen.is_frozen() {
                panic!("time is not frozen");
            }

            inner.base += duration;
        }

        pub(crate) fn now(&self) -> Instant {
            let inner = self.inner.lock();

            let mut ret = inner.base;

            if let Some(thawed) = inner.frozen.thawed() {
                ret += thawed.elapsed();
            }

            Instant::from_std(ret)
        }
    }
}
