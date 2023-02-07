use crate::time::{Clock, Duration, Instant};

use std::convert::TryInto;

/// A structure which handles conversion from Instants to u64 timestamps.
#[derive(Debug)]
pub(crate) struct TimeSource {
    start_time: Instant,
}

impl TimeSource {
    pub(crate) fn new(clock: &Clock) -> Self {
        Self {
            start_time: clock.now(),
        }
    }

    pub(crate) fn deadline_to_tick(&self, t: Instant) -> u64 {
        // Round up to the end of a ms
        self.instant_to_tick(t + Duration::from_nanos(999_999))
    }

    pub(crate) fn instant_to_tick(&self, t: Instant) -> u64 {
        // round up
        let dur: Duration = t
            .checked_duration_since(self.start_time)
            .unwrap_or_else(|| Duration::from_secs(0));
        let ms = dur.as_millis();

        ms.try_into().unwrap_or(u64::MAX)
    }

    pub(crate) fn tick_to_duration(&self, t: u64) -> Duration {
        Duration::from_millis(t)
    }

    pub(crate) fn now(&self, clock: &Clock) -> u64 {
        self.instant_to_tick(clock.now())
    }
}
