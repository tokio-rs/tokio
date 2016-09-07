use std::io;
use std::time::Instant;

use futures::task;

use reactor::{Message, Handle, Remote};

/// A token that identifies an active timeout.
pub struct TimeoutToken {
    token: usize,
    when: Instant,
}

impl TimeoutToken {
    /// Adds a new timeout to get fired at the specified instant, notifying the
    /// specified task.
    pub fn new(at: Instant, handle: &Handle) -> io::Result<TimeoutToken> {
        match handle.inner.upgrade() {
            Some(inner) => {
                let (token, when) = try!(inner.borrow_mut().add_timeout(at));
                Ok(TimeoutToken { token: token, when: when })
            }
            None => Err(io::Error::new(io::ErrorKind::Other, "event loop gone")),
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
    pub fn update_timeout(&self, handle: &Remote) {
        handle.send(Message::UpdateTimeout(self.token, task::park()))
    }

    /// Cancel a previously added timeout.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn cancel_timeout(&self, handle: &Remote) {
        debug!("cancel timeout {}", self.token);
        handle.send(Message::CancelTimeout(self.token))
    }
}
