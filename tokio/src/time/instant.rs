#![allow(clippy::trivially_copy_pass_by_ref)]

use std::fmt;
use std::ops;
use std::time::Duration;

/// A measurement of the system clock, useful for talking to
/// external entities like the file system or other processes.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd)]
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

    /// Convert the value into a `std::time::Instant`.
    pub fn into_std(self) -> std::time::Instant {
        self.std
    }

    /// Returns the amount of time elapsed from another instant to this one.
    ///
    /// # Panics
    ///
    /// This function will panic if `earlier` is later than `self`.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.std.duration_since(earlier.std)
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// None if that instant is later than this one.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::{Duration, Instant, delay_for};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let now = Instant::now();
    ///     delay_for(Duration::new(1, 0)).await;
    ///     let new_now = Instant::now();
    ///     println!("{:?}", new_now.checked_duration_since(now));
    ///     println!("{:?}", now.checked_duration_since(new_now)); // None
    /// }
    /// ```
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.std.checked_duration_since(earlier.std)
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// zero duration if that instant is earlier than this one.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::{Duration, Instant, delay_for};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let now = Instant::now();
    ///     delay_for(Duration::new(1, 0)).await;
    ///     let new_now = Instant::now();
    ///     println!("{:?}", new_now.saturating_duration_since(now));
    ///     println!("{:?}", now.saturating_duration_since(new_now)); // 0ns
    /// }
    /// ```
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.std.saturating_duration_since(earlier.std)
    }

    /// Returns the amount of time elapsed since this instant was created.
    ///
    /// # Panics
    ///
    /// This function may panic if the current time is earlier than this
    /// instant, which is something that can happen if an `Instant` is
    /// produced synthetically.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time::{Duration, Instant, delay_for};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let instant = Instant::now();
    ///     let three_secs = Duration::from_secs(3);
    ///     delay_for(three_secs).await;
    ///     assert!(instant.elapsed() >= three_secs);
    /// }
    /// ```
    pub fn elapsed(&self) -> Duration {
        Instant::now() - *self
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
        self.std - rhs.std
    }
}

impl ops::Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Instant {
        Instant::from_std(self.std - rhs)
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
