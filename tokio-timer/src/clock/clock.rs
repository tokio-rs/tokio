use clock::Now;
use timer;

use tokio_executor::Enter;

use std::cell::Cell;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

/// TODO: Dox
#[derive(Default, Clone)]
pub struct Clock {
    now: Option<Arc<Now>>,
}

/// Thread-local tracking the current clock
thread_local!(static CLOCK: Cell<Option<*const Clock>> = Cell::new(None));

/// TODO: Dox
pub fn now() -> Instant {
    CLOCK.with(|current| {
        match current.get() {
            Some(_) => unimplemented!(),
            None => Instant::now(),
        }
    })
}

impl Clock {
    /// TODO: Dox
    pub fn new() -> Clock {
        CLOCK.with(|current| {
            match current.get() {
                Some(_) => unimplemented!(),
                None => Clock::system(),
            }
        })
    }

    /// TODO: Dox
    pub fn new_with_now<T: Now>(now: T) -> Clock {
        Clock {
            now: Some(Arc::new(now)),
        }
    }

    /// TODO: Dox
    pub fn system() -> Clock {
        Clock {
            now: None,
        }
    }

    /// TODO: Docs
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

/// TODO: Dox
pub fn with_default<F, R>(clock: &Clock, enter: &mut Enter, f: F) -> R
where F: FnOnce(&mut Enter) -> R
{
    CLOCK.with(|cell| {
        assert!(cell.get().is_none(), "default clock already set for execution context");

        // Ensure that the clock is removed from the thread-local context
        // when leaving the scope. This handles cases that involve panicking.
        struct Reset<'a>(&'a Cell<Option<*const Clock>>);

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.0.set(None);
            }
        }

        let _reset = Reset(cell);

        cell.set(Some(clock as *const Clock));

        f(enter)
    })
}
