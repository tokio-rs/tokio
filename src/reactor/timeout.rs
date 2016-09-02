//! Support for creating futures that represent timeouts.
//!
//! This module contains the `Timeout` type which is a future that will resolve
//! at a particular point in the future.

use std::io;
use std::time::{Duration, Instant};

use futures::{Future, Poll, Async};

use reactor::Handle;
use reactor::timeout_token::TimeoutToken;
use io::IoFuture;

/// A future representing the notification that a timeout has occurred.
///
/// Timeouts are created through the `LoopHandle::timeout` or
/// `LoopHandle::timeout_at` methods indicating when a timeout should fire at.
/// Note that timeouts are not intended for high resolution timers, but rather
/// they will likely fire some granularity after the exact instant that they're
/// otherwise indicated to fire at.
pub struct Timeout {
    token: TimeoutToken,
    handle: Handle,
}

/// Future returned from `Timeout::new` and `Timeout::new_at` which will resolve
/// to the actual `Timeout` itself.
pub struct TimeoutNew {
    inner: IoFuture<Timeout>,
}

impl Timeout {
    /// Creates a new timeout which will fire at `dur` time into the future.
    ///
    /// This function will return a future that will resolve to the actual
    /// timeout object. The timeout object itself is then a future which will be
    /// set to fire at the specified point in the future.
    pub fn new(dur: Duration, handle: &Handle) -> TimeoutNew {
        Timeout::new_at(Instant::now() + dur, handle)
    }

    /// Creates a new timeout which will fire at the time specified by `at`.
    ///
    /// This function will return a future that will resolve to the actual
    /// timeout object. The timeout object itself is then a future which will be
    /// set to fire at the specified point in the future.
    pub fn new_at(at: Instant, handle: &Handle) -> TimeoutNew {
        let handle = handle.clone();
        TimeoutNew {
            inner: TimeoutToken::new(at, &handle).map(move |token| {
                Timeout {
                    token: token,
                    handle: handle,
                }
            }).boxed(),
        }
    }
}

impl Future for Timeout {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        // TODO: is this fast enough?
        let now = Instant::now();
        if *self.token.when() <= now {
            Ok(Async::Ready(()))
        } else {
            self.token.update_timeout(&self.handle);
            Ok(Async::NotReady)
        }
    }
}

impl Future for TimeoutNew {
    type Item = Timeout;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Timeout, io::Error> {
        self.inner.poll()
    }
}

impl Drop for Timeout {
    fn drop(&mut self) {
        self.token.cancel_timeout(&self.handle);
    }
}
