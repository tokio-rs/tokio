use std::sync::atomic::Ordering;
use std::io;

use mio::event::Evented;

use reactor::{Handle, Direction};

/// A token that identifies an active I/O resource.
pub struct IoToken {
    token: usize,
    handle: Handle,
}

impl IoToken {
    /// Add a new source to an event loop, returning a token that can be used to
    /// identify this source.
    ///
    /// When a new I/O object is created it needs to be communicated to the
    /// event loop to ensure that it's registered and ready to receive
    /// notifications. The event loop will then respond back with the I/O object
    /// and a token which can be used to send more messages to the event loop.
    ///
    /// The token returned is then passed in turn to each of the methods below
    /// to interact with notifications on the I/O object itself.
    ///
    /// # Panics
    ///
    /// The returned future will panic if the event loop this handle is
    /// associated with has gone away, or if there is an error communicating
    /// with the event loop.
    pub fn new(source: &Evented, handle: &Handle) -> io::Result<IoToken> {
        match handle.inner() {
            Some(inner) => {
                let token = try!(inner.add_source(source));
                let handle = handle.clone();

                Ok(IoToken { token, handle })
            }
            None => Err(io::Error::new(io::ErrorKind::Other, "event loop gone")),
        }
    }

    /// Returns a reference to this I/O token's event loop's handle.
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Consumes the last readiness notification the token this source is for
    /// registered.
    ///
    /// Currently sources receive readiness notifications on an edge-basis. That
    /// is, once you receive a notification that an object can be read, you
    /// won't receive any more notifications until all of that data has been
    /// read.
    ///
    /// The event loop will fill in this information and then inform futures
    /// that they're ready to go with the `schedule` method, and then the `poll`
    /// method can use this to figure out what happened.
    ///
    /// > **Note**: This method should generally not be used directly, but
    /// >           rather the `ReadinessStream` type should be used instead.
    // TODO: this should really return a proper newtype/enum, not a usize
    pub fn take_readiness(&self) -> usize {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return 0,
        };

        let io_dispatch = inner.io_dispatch.read().unwrap();
        io_dispatch[self.token].readiness.swap(0, Ordering::SeqCst)
    }

    /// Schedule the current future task to receive a notification when the
    /// corresponding I/O object is readable.
    ///
    /// Once an I/O object has been registered with the event loop through the
    /// `add_source` method, this method can be used with the assigned token to
    /// notify the current future task when the next read notification comes in.
    ///
    /// The current task will only receive a notification **once** and to
    /// receive further notifications it will need to call `schedule_read`
    /// again.
    ///
    /// > **Note**: This method should generally not be used directly, but
    /// >           rather the `ReadinessStream` type should be used instead.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    ///
    /// This function will also panic if there is not a currently running future
    /// task.
    pub fn schedule_read(&self) -> io::Result<()> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        inner.schedule(self.token, Direction::Read);
        Ok(())
    }

    /// Schedule the current future task to receive a notification when the
    /// corresponding I/O object is writable.
    ///
    /// Once an I/O object has been registered with the event loop through the
    /// `add_source` method, this method can be used with the assigned token to
    /// notify the current future task when the next write notification comes
    /// in.
    ///
    /// The current task will only receive a notification **once** and to
    /// receive further notifications it will need to call `schedule_write`
    /// again.
    ///
    /// > **Note**: This method should generally not be used directly, but
    /// >           rather the `ReadinessStream` type should be used instead.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    ///
    /// This function will also panic if there is not a currently running future
    /// task.
    pub fn schedule_write(&self) -> io::Result<()> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        inner.schedule(self.token, Direction::Write);
        Ok(())
    }

    /// Unregister all information associated with a token on an event loop,
    /// deallocating all internal resources assigned to the given token.
    ///
    /// This method should be called whenever a source of events is being
    /// destroyed. This will ensure that the event loop can reuse the `token`
    /// for another I/O object if necessary and also remove it from any poll
    /// notifications and callbacks.
    ///
    /// Note that wake callbacks may still be invoked after this method is
    /// called as it may take some time for the message to drop a source to
    /// reach the event loop. Despite this fact, this method will attempt to
    /// ensure that the callbacks are **not** invoked, so pending scheduled
    /// callbacks cannot be relied upon to get called.
    ///
    /// > **Note**: This method should generally not be used directly, but
    /// >           rather the `ReadinessStream` type should be used instead.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn drop_source(&self) {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return,
        };

        inner.drop_source(self.token)
    }
}
