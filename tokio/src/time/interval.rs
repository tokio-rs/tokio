use crate::future::poll_fn;
use crate::time::{sleep_until, Duration, Instant, Sleep};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Creates new `Interval` that yields with interval of `duration`. The first
/// tick completes immediately.
///
/// An interval will tick indefinitely. At any time, the `Interval` value can be
/// dropped. This cancels the interval.
///
/// This function is equivalent to `interval_at(Instant::now(), period)`.
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
///     interval.tick().await;
///     interval.tick().await;
///     interval.tick().await;
///
///     // approximately 20ms have elapsed.
/// }
/// ```
///
/// A simple example using `interval` to execute a task every two seconds.
///
/// The difference between `interval` and [`sleep`] is that an `interval`
/// measures the time since the last tick, which means that `.tick().await`
/// may wait for a shorter time than the duration specified for the interval
/// if some time has passed between calls to `.tick().await`.
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
pub fn interval(period: Duration) -> Interval {
    assert!(period > Duration::new(0, 0), "`period` must be non-zero.");

    interval_at(Instant::now(), period)
}

/// Creates new `Interval` that yields with interval of `period` with the
/// first tick completing at `start`.
///
/// An interval will tick indefinitely. At any time, the `Interval` value can be
/// dropped. This cancels the interval.
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
///     interval.tick().await;
///     interval.tick().await;
///     interval.tick().await;
///
///     // approximately 70ms have elapsed.
/// }
/// ```
pub fn interval_at(start: Instant, period: Duration) -> Interval {
    assert!(period > Duration::new(0, 0), "`period` must be non-zero.");

    Interval {
        delay: sleep_until(start),
        period,
    }
}

/// Stream returned by [`interval`](interval) and [`interval_at`](interval_at).
///
/// This type only implements the [`Stream`] trait if the "stream" feature is
/// enabled.
///
/// [`Stream`]: trait@crate::stream::Stream
#[derive(Debug)]
pub struct Interval {
    /// Future that completes the next time the `Interval` yields a value.
    delay: Sleep,

    /// The duration between values yielded by `Interval`.
    period: Duration,
}

impl Interval {
    fn poll_tick(&mut self, cx: &mut Context<'_>) -> Poll<Instant> {
        // Wait for the delay to be done
        ready!(Pin::new(&mut self.delay).poll(cx));

        // Get the `now` by looking at the `delay` deadline
        let now = self.delay.deadline();

        // The next interval value is `duration` after the one that just
        // yielded.
        let next = now + self.period;
        self.delay.reset(next);

        // Return the current instant
        Poll::Ready(now)
    }

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
}

#[cfg(feature = "stream")]
impl crate::stream::Stream for Interval {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Instant>> {
        Poll::Ready(Some(ready!(self.poll_tick(cx))))
    }
}
