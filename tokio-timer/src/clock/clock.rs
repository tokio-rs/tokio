use clock::Now;
use timer;

use tokio_executor::Enter;

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

/// A handle to a source of time.
///
/// `Clock` instances return [`Instant`] values corresponding to "now". The source
/// of these values is configurable. The default source is [`Instant::now`].
///
/// [`Instant`]: https://doc.rust-lang.org/std/time/struct.Instant.html
/// [`Instant::now`]: https://doc.rust-lang.org/std/time/struct.Instant.html#method.now
#[derive(Default, Clone)]
pub struct Clock {
    now: Option<Arc<dyn Now>>,
}

/// A guard that resets the current `Clock` to `None` when dropped.
#[derive(Debug)]
pub struct DefaultGuard {
    _p: (),
}

thread_local! {
    /// Thread-local tracking the current clock
    static CLOCK: RefCell<Option<Clock>> = RefCell::new(None)
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
/// # use tokio_timer::clock;
/// let now = clock::now();
/// ```
pub fn now() -> Instant {
    CLOCK.with(|current| match current.borrow().as_ref() {
        Some(c) => c.now(),
        None => Instant::now(),
    })
}

impl Clock {
    /// Return a new `Clock` instance that uses the current execution context's
    /// source of time.
    pub fn new() -> Clock {
        CLOCK.with(|current| match current.borrow().as_ref() {
            Some(c) => c.clone(),
            None => Clock::system(),
        })
    }

    /// Return a new `Clock` instance that uses `now` as the source of time.
    pub fn new_with_now<T: Now>(now: T) -> Clock {
        Clock {
            now: Some(Arc::new(now)),
        }
    }

    /// Return a new `Clock` instance that uses [`Instant::now`] as the source
    /// of time.
    ///
    /// [`Instant::now`]: https://doc.rust-lang.org/std/time/struct.Instant.html#method.now
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

#[allow(deprecated)]
impl timer::Now for Clock {
    fn now(&mut self) -> Instant {
        Clock::now(self)
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
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
pub fn with_default<F, R>(clock: &Clock, enter: &mut Enter, f: F) -> R
where
    F: FnOnce(&mut Enter) -> R,
{
    let _guard = set_default(clock);

    f(enter)
}

/// Sets `clock` as the default clock, returning a guard that unsets it on drop.
///
/// # Panics
///
/// This function panics if there already is a default clock set.
pub fn set_default(clock: &Clock) -> DefaultGuard {
    CLOCK.with(|cell| {
        assert!(
            cell.borrow().is_none(),
            "default clock already set for execution context"
        );

        *cell.borrow_mut() = Some(clock.clone());

        DefaultGuard { _p: () }
    })
}

impl Drop for DefaultGuard {
    fn drop(&mut self) {
        let _ = CLOCK.try_with(|cell| cell.borrow_mut().take());
    }
}
