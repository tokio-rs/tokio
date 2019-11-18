//! Source of time abstraction.
//!
//! By default, `std::time::Instant::now()` is used. However, when the
//! `test-util` feature flag is enabled, the values returned for `now()` are
//! configurable.

#[cfg(feature = "test-util")]
pub(crate) use self::variant::now;
pub(crate) use self::variant::Clock;
#[cfg(feature = "test-util")]
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::variant::{advance, pause, resume};

#[cfg(not(feature = "test-util"))]
mod variant {
    use crate::time::Instant;

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

        pub(crate) fn enter<F, R>(&self, f: F) -> R
        where
            F: FnOnce() -> R,
        {
            f()
        }
    }
}

#[cfg(feature = "test-util")]
mod variant {
    use crate::time::{Duration, Instant};

    use std::cell::Cell;
    use std::sync::{Arc, Mutex};

    /// A handle to a source of time.
    #[derive(Debug, Clone)]
    pub(crate) struct Clock {
        inner: Arc<Inner>,
    }

    #[derive(Debug)]
    struct Inner {
        /// Instant at which the clock was created
        start: std::time::Instant,

        /// Current, "frozen" time as an offset from `start`.
        frozen: Mutex<Option<Duration>>,
    }

    thread_local! {
        /// Thread-local tracking the current clock
        static CLOCK: Cell<Option<*const Clock>> = Cell::new(None)
    }

    /// Pause time
    ///
    /// The current value of `Instant::now()` is saved and all subsequent calls
    /// to `Instant::now()` will return the saved value. This is useful for
    /// running tests that are dependent on time.
    ///
    /// # Panics
    ///
    /// Panics if time is already frozen or if called from outside of the Tokio
    /// runtime.
    pub fn pause() {
        CLOCK.with(|cell| {
            let ptr = match cell.get() {
                Some(ptr) => ptr,
                None => panic!("time cannot be frozen from outside the Tokio runtime"),
            };

            let clock = unsafe { &*ptr };
            let mut frozen = clock.inner.frozen.lock().unwrap();

            if frozen.is_some() {
                panic!("time is already frozen");
            }

            *frozen = Some(clock.inner.start.elapsed());
        })
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
        CLOCK.with(|cell| {
            let ptr = match cell.get() {
                Some(ptr) => ptr,
                None => panic!("time cannot be frozen from outside the Tokio runtime"),
            };

            let clock = unsafe { &*ptr };
            let mut frozen = clock.inner.frozen.lock().unwrap();

            if frozen.is_none() {
                panic!("time is not frozen");
            }

            *frozen = None;
        })
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
        CLOCK.with(|cell| {
            let ptr = match cell.get() {
                Some(ptr) => ptr,
                None => panic!("time cannot be frozen from outside the Tokio runtime"),
            };

            let clock = unsafe { &*ptr };
            clock.advance(duration);
        });

        crate::task::yield_now().await;
    }

    /// Return the current instant, factoring in frozen time.
    pub(crate) fn now() -> Instant {
        CLOCK.with(|cell| {
            Instant::from_std(match cell.get() {
                Some(ptr) => {
                    let clock = unsafe { &*ptr };

                    if let Some(frozen) = *clock.inner.frozen.lock().unwrap() {
                        clock.inner.start + frozen
                    } else {
                        std::time::Instant::now()
                    }
                }
                None => std::time::Instant::now(),
            })
        })
    }

    impl Clock {
        /// Return a new `Clock` instance that uses the current execution context's
        /// source of time.
        pub(crate) fn new() -> Clock {
            Clock {
                inner: Arc::new(Inner {
                    start: std::time::Instant::now(),
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
                    start: std::time::Instant::now(),
                    frozen: Mutex::new(Some(Duration::from_millis(0))),
                }),
            }
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
            Instant::from_std(if let Some(frozen) = *self.inner.frozen.lock().unwrap() {
                self.inner.start + frozen
            } else {
                std::time::Instant::now()
            })
        }

        /// Set the clock as the default source of time for the duration of the
        /// closure
        pub(crate) fn enter<F, R>(&self, f: F) -> R
        where
            F: FnOnce() -> R,
        {
            CLOCK.with(|cell| {
                assert!(
                    cell.get().is_none(),
                    "default clock already set for execution context"
                );

                // Ensure that the clock is removed from the thread-local context
                // when leaving the scope. This handles cases that involve panicking.
                struct Reset<'a>(&'a Cell<Option<*const Clock>>);

                impl Drop for Reset<'_> {
                    fn drop(&mut self) {
                        self.0.set(None);
                    }
                }

                let _reset = Reset(cell);

                cell.set(Some(self as *const Clock));

                f()
            })
        }
    }
}
