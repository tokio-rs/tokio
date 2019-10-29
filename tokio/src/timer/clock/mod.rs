//! A configurable source of time.
//!
//! This module provides an API to get the current instant in such a way that
//! the source of time may be configured. This allows mocking out the source of
//! time in tests.
//!
//! The [`now`][n] function returns the current [`Instant`]. By default, it delegates
//! to [`Instant::now`].
//!
//! The source of time used by [`now`][n] can be configured by implementing the
//! [`Now`] trait and passing an instance to [`with_default`].
//!
//! [n]: fn.now.html
//! [`Now`]: trait.Now.html
//! [`Instant`]: std::time::Instant
//! [`Instant::now`]: std::time::Instant::now
//! [`with_default`]: fn.with_default.html

mod now;

pub use self::now::Now;

use std::cell::Cell;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

/// A handle to a source of time.
///
/// `Clock` instances return [`Instant`] values corresponding to "now". The source
/// of these values is configurable. The default source is [`Instant::now`].
///
/// [`Instant`]: std::time::Instant
/// [`Instant::now`]: std::time::Instant::now
#[derive(Default, Clone)]
pub struct Clock {
    now: Option<Arc<dyn Now>>,
}

thread_local! {
    /// Thread-local tracking the current clock
    static CLOCK: Cell<Option<*const Clock>> = Cell::new(None)
}

/// Returns an `Instant` corresponding to "now".
///
/// This function delegates to the source of time configured for the current
/// execution context. By default, this is `Instant::now()`.
///
/// Note that, because the source of time is configurable, it is possible to
/// observe non-monotonic behavior when calling `now` from different
/// executors.
///
/// See [module](index.html) level documentation for more details.
///
/// # Examples
///
/// ```
/// # use tokio::timer::clock;
/// let now = clock::now();
/// ```
pub fn now() -> Instant {
    CLOCK.with(|current| match current.get() {
        Some(ptr) => unsafe { (*ptr).now() },
        None => Instant::now(),
    })
}

impl Clock {
    /// Return a new `Clock` instance that uses the current execution context's
    /// source of time.
    pub fn new() -> Clock {
        CLOCK.with(|current| match current.get() {
            Some(ptr) => unsafe { (*ptr).clone() },
            None => Clock::system(),
        })
    }

    /// Return a new `Clock` instance that uses `now` as the source of time.
    pub fn new_with_now(now: impl Now) -> Clock {
        Clock {
            now: Some(Arc::new(now)),
        }
    }

    /// Return a new `Clock` instance that uses [`Instant::now`] as the source
    /// of time.
    ///
    /// [`Instant::now`]: std::time::Instant::now
    pub fn system() -> Clock {
        Clock { now: None }
    }

    /// Returns an instant corresponding to "now" by using the instance's source
    /// of time.
    pub fn now(&self) -> Instant {
        match self.now {
            Some(ref now) => now.now(),
            None => Instant::now(),
        }
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Clock")
            .field("now", {
                if self.now.is_some() {
                    &"Some(Arc<Now>)"
                } else {
                    &"None"
                }
            })
            .finish()
    }
}

/// Set the default clock for the duration of the closure.
///
/// # Panics
///
/// This function panics if there already is a default clock set.
pub fn with_default<F, R>(clock: &Clock, f: F) -> R
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

        cell.set(Some(clock as *const Clock));

        f()
    })
}
