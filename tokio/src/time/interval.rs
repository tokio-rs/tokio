use crate::future::poll_fn;
use crate::time::{sleep_until, Duration, Instant, Sleep};

use std::pin::Pin;
use std::task::{Context, Poll};
use std::{convert::TryInto, future::Future};

/// Creates new [`Interval`] that yields with interval of `period`. The first
/// tick completes immediately. The [`MissedTickBehavior`] is the
/// [default one](MissedTickBehavior::default), which is
/// [`Burst`](MissedTickBehavior::Burst).
///
/// An interval will tick indefinitely. At any time, the [`Interval`] value can
/// be dropped. This cancels the interval.
///
/// This function is equivalent to
/// [`interval_at(Instant::now(), period)`](interval_at).
///
/// # Panics
///
/// This function panics if `period` is zero.
///
/// # Examples
///
/// ```
/// use tokio::time::{self, Duration};
///
/// #[tokio::main]
/// async fn main() {
///     let mut interval = time::interval(Duration::from_millis(10));
///
///     interval.tick().await; // ticks immediately
///     interval.tick().await; // ticks after 10ms
///     interval.tick().await; // ticks after 10ms
///
///     // approximately 20ms have elapsed.
/// }
/// ```
///
/// A simple example using `interval` to execute a task every two seconds.
///
/// The difference between `interval` and [`sleep`] is that an `interval`
/// measures the time since the last tick, which means that [`.tick().await`]
/// may wait for a shorter time than the duration specified for the interval
/// if some time has passed between calls to [`.tick().await`].
///
/// If the tick in the example below was replaced with [`sleep`], the task
/// would only be executed once every three seconds, and not every two
/// seconds.
///
/// ```
/// use tokio::time;
///
/// async fn task_that_takes_a_second() {
///     println!("hello");
///     time::sleep(time::Duration::from_secs(1)).await
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let mut interval = time::interval(time::Duration::from_secs(2));
///     for _i in 0..5 {
///         interval.tick().await;
///         task_that_takes_a_second().await;
///     }
/// }
/// ```
///
/// [`sleep`]: crate::time::sleep()
/// [`.tick().await`]: Interval::tick
pub fn interval(period: Duration) -> Interval {
    assert!(period > Duration::new(0, 0), "`period` must be non-zero.");

    interval_at(Instant::now(), period)
}

/// Creates new [`Interval`] that yields with interval of `period` with the
/// first tick completing at `start`. The [`MissedTickBehavior`] is the
/// [default one](MissedTickBehavior::default), which is
/// [`Burst`](MissedTickBehavior::Burst).
///
/// An interval will tick indefinitely. At any time, the [`Interval`] value can
/// be dropped. This cancels the interval.
///
/// # Panics
///
/// This function panics if `period` is zero.
///
/// # Examples
///
/// ```
/// use tokio::time::{interval_at, Duration, Instant};
///
/// #[tokio::main]
/// async fn main() {
///     let start = Instant::now() + Duration::from_millis(50);
///     let mut interval = interval_at(start, Duration::from_millis(10));
///
///     interval.tick().await; // ticks after 50ms
///     interval.tick().await; // ticks after 10ms
///     interval.tick().await; // ticks after 10ms
///
///     // approximately 70ms have elapsed.
/// }
/// ```
pub fn interval_at(start: Instant, period: Duration) -> Interval {
    assert!(period > Duration::new(0, 0), "`period` must be non-zero.");

    Interval {
        delay: Box::pin(sleep_until(start)),
        period,
        missed_tick_behavior: Default::default(),
    }
}

/// Defines the behavior of an [`Interval`] when it misses a tick.
///
/// Occasionally, the executor may be blocked, and as a result, [`Interval`]
/// misses a tick. By default, when this happens, [`Interval`] fires ticks
/// as quickly as it can until it is back to where it should be. However,
/// this is not always the desired behavior when ticks are missed.
/// `MissedTickBehavior` allows users to specify which behavior they want
/// [`Interval`] to exhibit. Each variant represents a different strategy.
///
/// Because the executor cannot guarantee exect precision with
/// timers, these strategies will only apply when the
/// error in time is greater than 5 milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedTickBehavior {
    /// Tick as fast as possible until caught up.
    ///
    /// When this strategy is used, after regaining control, [`Interval`] ticks
    /// as fast as it can until it is caught up to where it should be had the
    /// executor not been blocked. The [`Instant`]s yielded by
    /// [`tick`](Interval::tick()) are the same as they would have been if the executor had not
    /// been blocked.
    ///
    /// This would look something like this:
    /// ```text
    /// Expected ticks: |     1     |     2     |     3     |     4     |     5     |     6     |
    /// Actual ticks:   | work -----|          delay          | work | work | work -| work -----|
    //  Poll behavior:  |   |       |                         |      |      |       |           |
    //                  |   |       |                         |      |      |       |           |
    //           Ready(s)   |       |             Ready(s + 2p)      |      |       |           |
    //                Pending       |                    Ready(s + 3p)      |       |           |
    //                   Ready(s + p)                           Ready(s + 4p)       |           |
    //                                                                  Ready(s + 5p)           |
    //                                                                              Ready(s + 6p)
    // * Where `s` is the start time and `p` is the period
    /// ```
    ///
    /// This is the default behavior when [`Interval`] is created with
    /// [`interval`] and [`interval_at`].
    ///
    /// Note: this is the behavior that [`Interval`] exhibited in previous
    /// versions of Tokio.
    Burst,

    /// Delay missed ticks to happen at multiples of `period` from when
    /// [`Interval`] regained control.
    ///
    /// When this strategy is used, after regaining control, [`Interval`] ticks
    /// immediately, then schedules all future ticks to happen at a regular
    /// `period` from the point when [`Interval`] regained control.
    /// [`tick`] yields immediately with an [`Instant`] that represents the time
    /// that the tick should have happened. All subsequent calls to
    /// [`tick`] yield an [`Instant`] that is a multiple of
    /// `period` away from the point that [`Interval`] regained control.
    ///
    /// This would look something like this:
    /// ```text
    /// Expected ticks: |     1     |     2     |     3     |     4     |     5     |     6     |
    /// Actual ticks:   | work -----|          delay          | work -----| work -----| work -----|
    //  Poll behavior:  |   |       |                         |   |       |           |           |
    //                  |   |       |                         |   |       |           |           |
    //           Ready(s)   |       |             Ready(s + 2p)   |       |           |           |
    //                Pending       |                       Pending       |           |           |
    //                   Ready(s + p)                     Ready(s + 2p + d)           |           |
    //                                                                Ready(s + 3p + d)           |
    //                                                                            Ready(s + 4p + d)
    // * Where `s` is the start time, `p` is the period, and `d` is the delay
    /// ```
    ///
    /// Note that as a result, the ticks are no longer guaranteed to happen at
    /// a multiple of `period` from `delay`.
    ///
    /// [`tick`]: Interval::tick
    Delay,

    /// Skip the missed ticks and tick on the next multiple of `period`.
    ///
    /// When this strategy is used, after regaining control, [`Interval`] ticks
    /// immediately, then schedules the next tick on the closest multiple of
    /// `period` from `delay`. Thus, the yielded value from
    /// [`tick`](Interval::tick) is an [`Instant`] that is some multiple of
    /// `period` from `start`. However, that yielded [`Instant`] is not
    /// guarenteed to be exactly one multiple of `period` from the tick that
    /// got blocked.
    ///
    /// This would look something like this:
    /// ```text
    /// Expected ticks: |     1     |     2     |     3     |     4     |     5     |     6     |
    /// Actual ticks:   | work -----|          delay          | work ---| work -----| work -----|
    //  Poll behavior:  |   |       |                         |         |           |           |
    //                  |   |       |                         |         |           |           |
    //           Ready(s)   |       |             Ready(s + 2p)         |           |           |
    //                Pending       |                       Ready(s + 4p)           |           |
    //                   Ready(s + p)                                   Ready(s + 5p)           |
    //                                                                              Ready(s + 6p)
    // * Where `s` is the start time and `p` is the period
    /// ```
    Skip,
}

impl MissedTickBehavior {
    /// Determine when the next tick should happen.
    fn next_timeout(&self, timeout: Instant, now: Instant, period: Duration) -> Instant {
        match self {
            Self::Burst => timeout + period,
            Self::Delay => now + period,
            Self::Skip => {
                now + period
                    - Duration::from_nanos(
                        ((now - timeout).as_nanos() % period.as_nanos())
                            .try_into()
                            // This operation is practically guaranteed not to
                            // fail, as in order for it to fail, `period` would
                            // have to be longer than `now - timeout`, and both
                            // would have to be longer than 584 years.
                            //
                            // If it did fail, there's not a good way to pass
                            // the error along to the user, so we just panic.
                            .expect(
                                "too much time has elapsed since the interval was supposed to tick",
                            ),
                    )
            }
        }
    }
}

impl Default for MissedTickBehavior {
    /// Returns [`MissedTickBehavior::Burst`].
    ///
    /// For most usecases, the [`Burst`] strategy is what is desired.
    /// Additionally, to preserve backwards compatibility, the [`Burst`]
    /// strategy must be the default. For these reasons,
    /// [`MissedTickBehavior::Burst`] is the default for
    /// [`MissedTickBehavior`]. See [`Burst`] for more details.
    ///
    /// [`Burst`]: MissedTickBehavior::Burst
    fn default() -> Self {
        Self::Burst
    }
}

/// Interval returned by [`interval`], [`interval_at`]
///
/// This type allows you to wait on a sequence of instants with a certain
/// duration between each instant. Unlike calling [`sleep`] in a loop, this lets
/// you count the time spent between the calls to [`sleep`] as well.
///
/// An `Interval` can be turned into a `Stream` with [`IntervalStream`].
///
/// [`IntervalStream`]: https://docs.rs/tokio-stream/latest/tokio_stream/wrappers/struct.IntervalStream.html
/// [`sleep`]: crate::time::sleep
#[derive(Debug)]
pub struct Interval {
    /// Future that completes the next time the `Interval` yields a value.
    delay: Pin<Box<Sleep>>,

    /// The duration between values yielded by `Interval`.
    period: Duration,

    /// The strategy `Interval` should use when the executor gets blocked and a
    /// tick is missed.
    missed_tick_behavior: MissedTickBehavior,
}

impl Interval {
    /// Completes when the next instant in the interval has been reached.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::time;
    ///
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut interval = time::interval(Duration::from_millis(10));
    ///
    ///     interval.tick().await;
    ///     interval.tick().await;
    ///     interval.tick().await;
    ///
    ///     // approximately 20ms have elapsed.
    /// }
    /// ```
    pub async fn tick(&mut self) -> Instant {
        poll_fn(|cx| self.poll_tick(cx)).await
    }

    /// Poll for the next instant in the interval to be reached.
    ///
    /// This method can return the following values:
    ///
    ///  * `Poll::Pending` if the next instant has not yet been reached.
    ///  * `Poll::Ready(instant)` if the next instant has been reached.
    ///
    /// When this method returns `Poll::Pending`, the current task is scheduled
    /// to receive a wakeup when the instant has elapsed. Note that on multiple
    /// calls to `poll_tick`, only the [`Waker`](std::task::Waker) from the
    /// [`Context`] passed to the most recent call is scheduled to receive a
    /// wakeup.
    pub fn poll_tick(&mut self, cx: &mut Context<'_>) -> Poll<Instant> {
        // Wait for the delay to be done
        ready!(Pin::new(&mut self.delay).poll(cx));

        // Get the time at which the `delay` was supposed to complete
        let timeout = self.delay.deadline();

        let now = Instant::now();

        // If the executor was not blocked, and thus we are being called
        // before the next tick is due, just schedule the next tick normally,
        // one `period` after `scheduled_timeout`, when the last tick went
        // off
        //
        // However, if a tick took excessively long and we are now behind,
        // schedule the next tick according to how the user specified with
        // `MissedTickBehavior`
        let next = if now > timeout + Duration::from_millis(5) {
            self.missed_tick_behavior
                .next_timeout(timeout, now, self.period)
        } else {
            timeout + self.period
        };

        self.delay.as_mut().reset(next);

        // Return the current instant
        Poll::Ready(timeout)
    }

    /// Returns the [`MissedTickBehavior`] strategy that is currently being
    /// used.
    pub fn missed_tick_behavior(&self) -> MissedTickBehavior {
        self.missed_tick_behavior
    }

    /// Sets the [`MissedTickBehavior`] strategy that should be used.
    pub fn set_missed_tick_behavior(&mut self, behavior: MissedTickBehavior) {
        self.missed_tick_behavior = behavior;
    }

    /// Resets the [`MissedTickBehavior`] strategy that should be used to the
    /// [default one](MissedTickBehavior::default), which is
    /// [`Burst`](MissedTickBehavior::Burst).
    pub fn reset_missed_tick_behavior(&mut self) {
        self.missed_tick_behavior = Default::default();
    }

    /// Returns the period of the interval.
    pub fn period(&self) -> Duration {
        self.period
    }
}
