use std::io;
use std::time::Instant;

use futures::{Future, Poll};
use futures::task;

use event_loop::{Message, Loop, LoopHandle, LoopFuture};

impl LoopHandle {
    /// Adds a new timeout to get fired at the specified instant, notifying the
    /// specified task.
    pub fn add_timeout(&self, at: Instant) -> AddTimeout {
        AddTimeout {
            inner: LoopFuture {
                loop_handle: self.clone(),
                data: Some(at),
                result: None,
            },
        }
    }

    /// Updates a previously added timeout to notify a new task instead.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn update_timeout(&self, timeout: &TimeoutToken) {
        self.send(Message::UpdateTimeout(timeout.token, task::park()))
    }

    /// Cancel a previously added timeout.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn cancel_timeout(&self, timeout: &TimeoutToken) {
        debug!("cancel timeout {}", timeout.token);
        self.send(Message::CancelTimeout(timeout.token))
    }
}

/// Return value from the [`LoopHandle::add_timeout`] method, a future that will
/// resolve to a `TimeoutToken` to configure the behavior of that timeout.
///
/// [`LoopHandle::add_timeout`]: struct.LoopHandle.html#method.add_timeout
pub struct AddTimeout {
    inner: LoopFuture<(usize, Instant), Instant>,
}

/// A token that identifies an active timeout.
pub struct TimeoutToken {
    token: usize,
    when: Instant,
}

impl Future for AddTimeout {
    type Item = TimeoutToken;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<TimeoutToken, io::Error> {
        let (t, i) = try_ready!(self.inner.poll(Loop::add_timeout,
                                               Message::AddTimeout));
        Ok(TimeoutToken {
            token: t,
            when: i,
        }.into())
    }
}

impl TimeoutToken {
    /// Returns the instant in time when this timeout token will "fire".
    ///
    /// Note that this instant may *not* be the instant that was passed in when
    /// the timeout was created. The event loop does not support high resolution
    /// timers, so the exact resolution of when a timeout may fire may be
    /// slightly fudged.
    pub fn when(&self) -> &Instant {
        &self.when
    }
}

