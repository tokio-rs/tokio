//! This module contains a type that is essentially a (u16, Duration) pair,
//! including an atomic container for this pair type. Supports durations up to
//! 78 hours.
//!
//! The u16 counter can be used to determine if the duration has changed since
//! last time you look at it.
use crate::loom::sync::atomic::{AtomicU64, Ordering};
use std::fmt;
use std::time::Duration;

const MAX_NANOS: u64 = (1u64 << 48) - 1;
const NANOS_MASK: u64 = MAX_NANOS;
const COUNTER_MASK: u64 = !NANOS_MASK;
const COUNTER_ONE: u64 = 1u64 << 48;

#[derive(Copy, Clone, Default)]
pub(crate) struct CounterDuration {
    value: u64,
}

impl CounterDuration {
    #[cfg(test)]
    pub(crate) fn new(counter: u16, duration: Duration) -> Self {
        let nanos = std::cmp::min(duration.as_nanos(), u128::from(MAX_NANOS)) as u64;
        Self {
            value: (u64::from(counter) << 48) | nanos,
        }
    }

    pub(crate) fn counter(self) -> u16 {
        (self.value >> 48) as u16
    }

    pub(crate) fn duration(self) -> Duration {
        Duration::from_nanos(self.value & MAX_NANOS)
    }

    /// Increment the counter by one and replace the duration with the supplied
    /// duration.
    pub(crate) fn set_next_duration(&mut self, dur: Duration) {
        let nanos = std::cmp::min(dur.as_nanos(), u128::from(MAX_NANOS)) as u64;
        let counter_bits = (self.value & COUNTER_MASK).wrapping_add(COUNTER_ONE);
        self.value = counter_bits | nanos;
    }

    pub(crate) fn into_pair(self) -> (u16, Duration) {
        (self.counter(), self.duration())
    }
}

#[derive(Default)]
pub(crate) struct AtomicCounterDuration {
    value: AtomicU64,
}

impl AtomicCounterDuration {
    pub(crate) fn store(&self, new_value: CounterDuration, ordering: Ordering) {
        self.value.store(new_value.value, ordering);
    }

    pub(crate) fn load(&self, ordering: Ordering) -> CounterDuration {
        CounterDuration {
            value: self.value.load(ordering),
        }
    }
}

impl fmt::Debug for CounterDuration {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("CounterDuration")
            .field("counter", &self.counter())
            .field("duration", &self.duration())
            .finish()
    }
}

impl fmt::Debug for AtomicCounterDuration {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.load(Ordering::Relaxed);
        fmt.debug_struct("AtomicCounterDuration")
            .field("counter", &value.counter())
            .field("duration", &value.duration())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_increment() {
        let mut dur = 1u64;
        let mut cd = CounterDuration::new(u16::MAX, Duration::from_nanos(dur));

        for counter in 0..(1u32 << 18) {
            // Multiply by a prime number to get a sequence of mostly unrelated
            // durations.
            dur = (dur * 32717) % (1 + MAX_NANOS);
            cd.set_next_duration(Duration::from_nanos(dur));

            // Note that `counter as u16` will truncate extra bits. This is
            // intended.
            assert_eq!(cd.counter(), counter as u16);
            assert_eq!(cd.duration().as_nanos(), u128::from(dur));
        }
    }
}
