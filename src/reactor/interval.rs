//! Support for creating futures that represent intervals.
//!
//! This module contains the `Interval` type which is a stream that will
//! resolve at a fixed intervals in future

use std::io;
use std::time::{Duration, Instant};

use futures::{Poll, Async};
use futures::stream::{Stream};

use reactor::{Remote, Handle};
use reactor::timeout_token::TimeoutToken;

/// A stream representing notifications at fixed interval
///
/// Intervals are created through the `Interval::new` or
/// `Interval::new_at` methods indicating when a first notification
/// should be triggered and when it will be repeated.
///
/// Note that timeouts are not intended for high resolution timers, but rather
/// they will likely fire some granularity after the exact instant that they're
/// otherwise indicated to fire at.
#[must_use = "streams do nothing unless polled"]
pub struct Interval {
    token: TimeoutToken,
    next: Instant,
    interval: Duration,
    handle: Remote,
}

impl Interval {
    /// Creates a new interval which will fire at `dur` time into the future,
    /// and will repeat every `dur` interval after
    ///
    /// This function will return a future that will resolve to the actual
    /// interval object. The interval object itself is then a stream which will
    /// be set to fire at the specified intervals
    pub fn new(dur: Duration, handle: &Handle) -> io::Result<Interval> {
        Interval::new_at(Instant::now() + dur, dur, handle)
    }

    /// Creates a new interval which will fire at the time specified by `at`,
    /// and then will repeat every `dur` interval after
    ///
    /// This function will return a future that will resolve to the actual
    /// timeout object. The timeout object itself is then a future which will be
    /// set to fire at the specified point in the future.
    pub fn new_at(at: Instant, dur: Duration, handle: &Handle)
        -> io::Result<Interval>
    {
        Ok(Interval {
            token: try!(TimeoutToken::new(at, &handle)),
            next: at,
            interval: dur,
            handle: handle.remote().clone(),
        })
    }

    /// Polls this `Interval` instance to see if it's elapsed, assuming the
    /// current time is specified by `now`.
    ///
    /// The `Future::poll` implementation for `Interval` will call `Instant::now`
    /// each time it's invoked, but in some contexts this can be a costly
    /// operation. This method is provided to amortize the cost by avoiding
    /// usage of `Instant::now`, assuming that it's been called elsewhere.
    ///
    /// This function takes the assumed current time as the first parameter and
    /// otherwise functions as this future's `poll` function. This will block a
    /// task if one isn't already blocked or update a previous one if already
    /// blocked.
    fn poll_at(&mut self, now: Instant) -> Poll<Option<()>, io::Error> {
        if self.next <= now {
            self.next = next_interval(self.next, now, self.interval);
            self.token.reset_timeout(self.next, &self.handle);
            Ok(Async::Ready(Some(())))
        } else {
            self.token.update_timeout(&self.handle);
            Ok(Async::NotReady)
        }
    }
}

impl Stream for Interval {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<()>, io::Error> {
        // TODO: is this fast enough?
        self.poll_at(Instant::now())
    }
}

impl Drop for Interval {
    fn drop(&mut self) {
        self.token.cancel_timeout(&self.handle);
    }
}

/// Converts Duration object to raw nanoseconds if possible
///
/// This is useful to divide intervals.
///
/// While technically for large duration it's impossible to represent any
/// duration as nanoseconds, the largest duration we can represent is about
/// 427_000 years. Large enough for any interval we would use or calculate in
/// tokio.
fn duration_to_nanos(dur: Duration) -> Option<u64> {
    dur.as_secs()
        .checked_mul(1_000_000_000)
        .and_then(|v| v.checked_add(dur.subsec_nanos() as u64))
}

fn next_interval(prev: Instant, now: Instant, interval: Duration) -> Instant {
    let new = prev + interval;
    if new > now {
        return new;
    } else {
        let spent_ns = duration_to_nanos(now.duration_since(prev))
            .expect("interval should be expired");
        let interval_ns = duration_to_nanos(interval)
            .expect("interval is less that 427 thousand years");
        let mult = spent_ns/interval_ns + 1;
        assert!(mult < (1 << 32),
            "can't skip more than 4 billion intervals of {:?} \
             (trying to skip {})", interval, mult);
        return prev + interval * (mult as u32);
    }
}

#[cfg(test)]
mod test {
    use std::time::{Instant, Duration};
    use super::next_interval;

    struct Timeline(Instant);

    impl Timeline {
        fn new() -> Timeline {
            Timeline(Instant::now())
        }
        fn at(&self, millis: u64) -> Instant {
            self.0 + Duration::from_millis(millis)
        }
        fn at_ns(&self, sec: u64, nanos: u32) -> Instant {
            self.0 + Duration::new(sec, nanos)
        }
    }

    fn dur(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    // The math around Instant/Duration isn't 100% precise due to rounding
    // errors, see #249 for more info
    fn almost_eq(a: Instant, b: Instant) -> bool {
        if a == b {
            true
        } else if a > b {
            a - b < Duration::from_millis(1)
        } else {
            b - a < Duration::from_millis(1)
        }
    }

    #[test]
    fn norm_next() {
        let tm = Timeline::new();
        assert!(almost_eq(next_interval(tm.at(1), tm.at(2), dur(10)),
                                        tm.at(11)));
        assert!(almost_eq(next_interval(tm.at(7777), tm.at(7788), dur(100)),
                                        tm.at(7877)));
        assert!(almost_eq(next_interval(tm.at(1), tm.at(1000), dur(2100)),
                                        tm.at(2101)));
    }

    #[test]
    fn fast_forward() {
        let tm = Timeline::new();
        assert!(almost_eq(next_interval(tm.at(1), tm.at(1000), dur(10)),
                                        tm.at(1001)));
        assert!(almost_eq(next_interval(tm.at(7777), tm.at(8888), dur(100)),
                                        tm.at(8977)));
        assert!(almost_eq(next_interval(tm.at(1), tm.at(10000), dur(2100)),
                                        tm.at(10501)));
    }

    /// TODO: this test actually should be successful, but since we can't
    ///       multiply Duration on anything larger than u32 easily we decided
    ///       to allow it to fail for now
    #[test]
    #[should_panic(expected = "can't skip more than 4 billion intervals")]
    fn large_skip() {
        let tm = Timeline::new();
        assert_eq!(next_interval(
            tm.at_ns(0, 1), tm.at_ns(25, 0), Duration::new(0, 2)),
            tm.at_ns(25, 1));
    }

}
