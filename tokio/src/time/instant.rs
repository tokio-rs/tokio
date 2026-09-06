#![allow(clippy::trivially_copy_pass_by_ref)]

use std::time::Duration;
use std::{fmt, ops, sync};

/// A measurement of a monotonically nondecreasing clock.
/// Opaque and useful only with `Duration`.
///
/// Instants are always guaranteed to be no less than any previously measured
/// instant when created, and are often useful for tasks such as measuring
/// benchmarks or timing how long an operation takes.
///
/// Note, however, that instants are not guaranteed to be **steady**. In other
/// words, each tick of the underlying clock may not be the same length (e.g.
/// some seconds may be longer than others). An instant may jump forwards or
/// experience time dilation (slow down or speed up), but it will never go
/// backwards.
///
/// Instants are opaque types that can only be compared to one another. There is
/// no method to get "the number of seconds" from an instant. Instead, it only
/// allows measuring the duration between two instants (or comparing two
/// instants).
///
/// The size of an `Instant` struct may vary depending on the target operating
/// system.
///
/// `Instant` offers non-panicking alternatives to `Add<Duration>`, etc, that
/// should be used where possible.
///
/// # Note
///
/// This type wraps the inner `std` variant and is used to align the Tokio
/// clock for uses of `now()`. This can be useful for testing where you can
/// take advantage of `time::pause()` and `time::advance()`.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Instant {
    std: std::time::Instant,
}

impl Instant {
    /// Returns an instant corresponding to "now".
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::Instant;
    ///
    /// let now = Instant::now();
    /// ```
    pub fn now() -> Instant {
        variant::now()
    }

    /// Create a `tokio::time::Instant` from a `std::time::Instant`.
    pub fn from_std(std: std::time::Instant) -> Instant {
        Instant { std }
    }

    /// The maximum instant.
    ///
    /// How far in the future this is varies by platform.
    pub(crate) fn max() -> Instant {
        *INSTANT_MAX
    }

    /// Convert the value into a `std::time::Instant`.
    pub fn into_std(self) -> std::time::Instant {
        self.std
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// zero duration if that instant is later than this one.
    ///
    /// Equivalent to [`Self::saturating_duration_since`].
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.std.saturating_duration_since(earlier.std)
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// None if that instant is later than this one.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::{Duration, Instant, sleep};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let now = Instant::now();
    /// sleep(Duration::new(1, 0)).await;
    /// let new_now = Instant::now();
    /// println!("{:?}", new_now.checked_duration_since(now));
    /// println!("{:?}", now.checked_duration_since(new_now)); // None
    /// # }
    /// ```
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.std.checked_duration_since(earlier.std)
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// zero duration if that instant is later than this one.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::{Duration, Instant, sleep};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let now = Instant::now();
    /// sleep(Duration::new(1, 0)).await;
    /// let new_now = Instant::now();
    /// println!("{:?}", new_now.saturating_duration_since(now));
    /// println!("{:?}", now.saturating_duration_since(new_now)); // 0ns
    /// }
    /// ```
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.std.saturating_duration_since(earlier.std)
    }

    /// Returns the amount of time elapsed since this instant was created,
    /// or zero duration if this instant is in the future.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::{Duration, Instant, sleep};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let instant = Instant::now();
    /// let three_secs = Duration::from_secs(3);
    /// sleep(three_secs).await;
    /// assert!(instant.elapsed() >= three_secs);
    /// # }
    /// ```
    pub fn elapsed(&self) -> Duration {
        Instant::now().saturating_duration_since(*self)
    }

    /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be
    /// represented as `Instant` (which means it's inside the bounds of the
    /// underlying data structure), `None` otherwise.
    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.std.checked_add(duration).map(Instant::from_std)
    }

    /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be
    /// represented as `Instant` (which means it's inside the bounds of the
    /// underlying data structure), `None` otherwise.
    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        self.std.checked_sub(duration).map(Instant::from_std)
    }

    /// Returns `self` advanced by `duration`, unless that would overflow the underlying
    /// representation used by `Instant`, in which case the maximum `Instant` is returned.
    pub fn saturating_add(&self, duration: Duration) -> Self {
        self.checked_add(duration).unwrap_or_else(Self::max)
    }
}

impl From<std::time::Instant> for Instant {
    fn from(time: std::time::Instant) -> Instant {
        Instant::from_std(time)
    }
}

impl From<Instant> for std::time::Instant {
    fn from(time: Instant) -> std::time::Instant {
        time.into_std()
    }
}

impl ops::Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, other: Duration) -> Instant {
        Instant::from_std(self.std + other)
    }
}

impl ops::AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl ops::Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Duration {
        self.std.saturating_duration_since(rhs.std)
    }
}

impl ops::Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Instant {
        Instant::from_std(std::time::Instant::sub(self.std, rhs))
    }
}

impl ops::SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.std.fmt(fmt)
    }
}

/// The biggest possible `std::time::Instant`.
///
/// Calculating this once at startup is reasonable since at least on Linux `Instant`
/// is backed by a nanosecond counter since boot.
static INSTANT_MAX: sync::LazyLock<Instant> =
    sync::LazyLock::new(|| find_max_instant(std::time::Instant::now()).into());

fn find_max_instant(start: std::time::Instant) -> std::time::Instant {
    // the result of the most recent guess that worked
    let mut max_instant = start;

    // lo is always valid
    let mut lo = Duration::ZERO;
    // hi is never valid
    let mut hi = match start.checked_add(Duration::MAX) {
        None => Duration::MAX,
        Some(max) => {
            // To maintain the invariant that hi is invalid, we return immediately.
            // Maybe some platform has a max expressible Instant greater than this, but
            // if so, 584 billion years in the future is probably far enough.
            // If this is ever hit, the test ensuring that adding 1ns exceeds Instant
            // would fail.
            return max.into();
        }
    };
    // may or may not be valid
    let mut guess = hi / 2;

    // Binary search for the latest Instant possible on the current platform.
    // With 64 bit seconds and ~30 bit nanoseconds, this should take at most 94 rounds.
    while guess > lo {
        match start.checked_add(guess) {
            None => {
                hi = guess;
            }
            Some(new_max) => {
                max_instant = new_max;
                lo = guess;
            }
        }
        // Subtraction can't underflow since hi > lo.
        let delta = hi - lo;
        // If delta = 1ns, delta / 2 is 0, so guess == lo.
        // This will break out of the loop.
        guess = lo + (delta) / 2;

        debug_assert!(hi > lo);
        debug_assert!(hi >= guess);
        debug_assert!(guess >= lo);
        debug_assert!(start.checked_add(lo).is_some());
        debug_assert!(start.checked_add(hi).is_none());

        println!("hi {hi:?} lo {lo:?} guess {guess:?}");
    }

    // Since guess == lo, hi - lo = 1ns, and lo is the biggest possible duration.
    // `max_instant` was written to when `lo` was, so we can return that without
    // further calculation.

    max_instant.into()
}

#[cfg(not(feature = "test-util"))]
mod variant {
    use super::Instant;

    pub(super) fn now() -> Instant {
        Instant::from_std(std::time::Instant::now())
    }
}

#[cfg(feature = "test-util")]
mod variant {
    use super::Instant;

    pub(super) fn now() -> Instant {
        crate::time::clock::now()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use super::*;

    /// Ensure there isn't pathological behavior in the binary search
    #[test]
    fn find_max_across_various_starts() {
        let start = std::time::Instant::now();
        let maxes = (0..100)
            .flat_map(|secs| (1..100).map(move |nanos| (secs, nanos)))
            .map(|(secs, nanos)| start + Duration::new(secs, nanos))
            .map(find_max_instant)
            .collect::<Vec<_>>();

        let maxes_dedup = maxes.iter().collect::<HashSet<_>>();
        assert_eq!(1, maxes_dedup.len(), "maxes: {maxes_dedup:?}");
    }

    /// Confirm `max` works on all supported platforms
    #[test]
    fn instant_max_cant_be_added_to() {
        let max = Instant::max();
        // confirm we found the max
        assert_eq!(
            None,
            max.checked_add(Duration::from_nanos(1)),
            "max: {:?} in the future",
            max.into_std().duration_since(std::time::Instant::now())
        );
    }
}
