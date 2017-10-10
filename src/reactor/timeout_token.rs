use std::io;
use std::time::Instant;

use futures::task;

use reactor::{Message, Handle, Remote};

/// A token that identifies an active timeout.
#[derive(Debug)]
pub struct TimeoutToken {
    token: usize,
}

impl TimeoutToken {
    /// Adds a new timeout to get fired at the specified instant, notifying the
    /// specified task.
    pub fn new(at: Instant, handle: &Handle) -> io::Result<TimeoutToken> {
        match handle.inner.upgrade() {
            Some(inner) => {
                let token = inner.borrow_mut().add_timeout(at);
                Ok(TimeoutToken { token: token })
            }
            None => Err(io::Error::new(io::ErrorKind::Other, "event loop gone")),
        }
    }

    /// Updates a previously added timeout to notify a new task instead.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn update_timeout(&self, handle: &Remote) {
        handle.send(Message::UpdateTimeout(self.token, task::current()))
    }

    /// Resets previously added (or fired) timeout to an new timeout
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn reset_timeout(&mut self, at: Instant, handle: &Remote) {
        handle.send(Message::ResetTimeout(self.token, at));
    }

    /// Cancel a previously added timeout.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method or if called multiple times.
    pub fn cancel_timeout(&self, handle: &Remote) {
        debug!("cancel timeout {}", self.token);
        handle.send(Message::CancelTimeout(self.token))
    }
}
