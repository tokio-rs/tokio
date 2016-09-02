use std::io;
use std::time::Instant;

use futures::{Future, Poll};
use futures::task;

use reactor::{Message, Core, Handle, CoreFuture};

/// Return value from the `Handle::add_timeout` method, a future that will
/// resolve to a `TimeoutToken` to configure the behavior of that timeout.
pub struct TimeoutTokenNew {
    inner: CoreFuture<(usize, Instant), Instant>,
}

/// A token that identifies an active timeout.
pub struct TimeoutToken {
    token: usize,
    when: Instant,
}

impl TimeoutToken {
    /// Adds a new timeout to get fired at the specified instant, notifying the
    /// specified task.
    pub fn new(at: Instant, handle: &Handle) -> TimeoutTokenNew {
        TimeoutTokenNew {
            inner: CoreFuture {
                handle: handle.clone(),
                data: Some(at),
                result: None,
            },
        }
    }

    /// Returns the instant in time when this timeout token will "fire".
    ///
    /// Note that this instant may *not* be the instant that was passed in when
    /// the timeout was created. The event loop does not support high resolution
    /// timers, so the exact resolution of when a timeout may fire may be
    /// slightly fudged.
    pub fn when(&self) -> &Instant {
        &self.when
    }

    /// Updates a previously added timeout to notify a new task instead.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn update_timeout(&self, handle: &Handle) {
        handle.send(Message::UpdateTimeout(self.token, task::park()))
    }

    /// Cancel a previously added timeout.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn cancel_timeout(&self, handle: &Handle) {
        debug!("cancel timeout {}", self.token);
        handle.send(Message::CancelTimeout(self.token))
    }
}

impl Future for TimeoutTokenNew {
    type Item = TimeoutToken;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<TimeoutToken, io::Error> {
        let (t, i) = try_ready!(self.inner.poll(Core::add_timeout,
                                                Message::AddTimeout));
        Ok(TimeoutToken {
            token: t,
            when: i,
        }.into())
    }
}
