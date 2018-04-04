// TODO: Remove this when finished
#![allow(dead_code, unused_imports, missing_docs)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_executor::park::{Park, Unpark};

#[derive(Debug, Clone)]
pub struct Clock(Arc<Mutex<Instant>>);

impl Clock {
    pub fn new() -> Clock {
        Clock(Arc::new(Mutex::new(Instant::now())))
    }

    pub fn sleep(&self, duration: Duration) {
        let mut time = self.0.lock().expect("Clock's mutex was poisoned");
        *time += duration;
    }

    pub fn now(&self) -> Instant {
        self.0.lock().expect("Clock's mutex was poisoned").clone()
    }
}

/// A `Park` implementation which will return immediately, while allowing time
/// on the underlying `Clock` to progress.
#[derive(Debug, Clone)]
pub struct NopPark(Clock);

impl NopPark {
    pub(crate) fn new(clock: Clock) -> NopPark {
        NopPark(clock.clone())
    }
}

/// By default whenever we `park()` the current thread, the clock should advance
/// by 1000ns (1us).
const PARK_DELAY: u32 = 1_000;

impl Park for NopPark {
    type Unpark = NopUnpark;
    type Error = ();

    fn park(&mut self) -> Result<(), Self::Error> {
        self.0.sleep(Duration::new(0, PARK_DELAY));
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.0.sleep(duration);
        Ok(())
    }

    fn unpark(&self) -> Self::Unpark {
        NopUnpark
    }
}

/// An `Unpark` which does nothing.
#[derive(Debug, Clone)]
pub struct NopUnpark;

impl Unpark for NopUnpark {
    fn unpark(&self) {
        // a NopPark will never block, so there's no unparking to be done.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_can_sleep() {
        let c = Clock::new();
        let start = c.now();

        let dur = Duration::new(1, 0);
        let should_be = start + dur;

        c.sleep(dur);
        let got = c.now();
        assert_eq!(got, should_be);
    }

    /// This ensures "time" will always progress while the current thread is
    /// parked, preventing accidental deadlocks.
    #[test]
    fn nop_park_always_progresses_time() {
        let c = Clock::new();
        let mut nop = NopPark::new(c.clone());

        let before = c.now();
        nop.park().unwrap();
        let after = c.now();

        assert!(after > before);
    }

    #[test]
    fn nop_park_timeout_progresses_by_exactly_the_duration() {
        let c = Clock::new();
        let start = c.now();

        let dur = Duration::new(1, 0);
        let should_be = start + dur;

        let mut nop = NopPark::new(c.clone());
        nop.park_timeout(dur).unwrap();

        let got = c.now();
        assert_eq!(got, should_be);
    }
}
