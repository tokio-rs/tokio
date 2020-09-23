use crate::io::driver::{Direction, Handle, ReadyEvent, ScheduledIo};
use crate::util::slab;

use mio::{self, Evented};
use std::io;
use std::task::{Context, Poll};

cfg_io_driver! {
    /// Associates an I/O resource with the reactor instance that drives it.
    ///
    /// A registration represents an I/O resource registered with a Reactor such
    /// that it will receive task notifications on readiness. This is the lowest
    /// level API for integrating with a reactor.
    ///
    /// The association between an I/O resource is made by calling [`new`]. Once
    /// the association is established, it remains established until the
    /// registration instance is dropped.
    ///
    /// A registration instance represents two separate readiness streams. One
    /// for the read readiness and one for write readiness. These streams are
    /// independent and can be consumed from separate tasks.
    ///
    /// **Note**: while `Registration` is `Sync`, the caller must ensure that
    /// there are at most two tasks that use a registration instance
    /// concurrently. One task for [`poll_read_ready`] and one task for
    /// [`poll_write_ready`]. While violating this requirement is "safe" from a
    /// Rust memory safety point of view, it will result in unexpected behavior
    /// in the form of lost notifications and tasks hanging.
    ///
    /// ## Platform-specific events
    ///
    /// `Registration` also allows receiving platform-specific `mio::Ready`
    /// events. These events are included as part of the read readiness event
    /// stream. The write readiness event stream is only for `Ready::writable()`
    /// events.
    ///
    /// [`new`]: method@Self::new
    /// [`poll_read_ready`]: method@Self::poll_read_ready`
    /// [`poll_write_ready`]: method@Self::poll_write_ready`
    #[derive(Debug)]
    pub(crate) struct Registration {
        /// Handle to the associated driver.
        handle: Handle,

        /// Reference to state stored by the driver.
        shared: slab::Ref<ScheduledIo>,
    }
}

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

// ===== impl Registration =====

impl Registration {
    /// Registers the I/O resource with the default reactor, for a specific `mio::Ready` state.
    /// `new_with_ready` should be used over `new` when you need control over the readiness state,
    /// such as when a file descriptor only allows reads. This does not add `hup` or `error` so if
    /// you are interested in those states, you will need to add them to the readiness state passed
    /// to this function.
    ///
    /// An example to listen to read only
    ///
    /// ```rust
    /// ##[cfg(unix)]
    ///     mio::Ready::from_usize(
    ///         mio::Ready::readable().as_usize()
    ///         | mio::unix::UnixReady::error().as_usize()
    ///         | mio::unix::UnixReady::hup().as_usize()
    ///     );
    /// ```
    ///
    /// # Return
    ///
    /// - `Ok` if the registration happened successfully
    /// - `Err` if an error was encountered during registration
    pub(crate) fn new_with_ready_and_handle<T>(
        io: &T,
        ready: mio::Ready,
        handle: Handle,
    ) -> io::Result<Registration>
    where
        T: Evented,
    {
        let shared = if let Some(inner) = handle.inner() {
            inner.add_source(io, ready)?
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to find event loop",
            ));
        };

        Ok(Registration { handle, shared })
    }

    /// Deregisters the I/O resource from the reactor it is associated with.
    ///
    /// This function must be called before the I/O resource associated with the
    /// registration is dropped.
    ///
    /// Note that deregistering does not guarantee that the I/O resource can be
    /// registered with a different reactor. Some I/O resource types can only be
    /// associated with a single reactor instance for their lifetime.
    ///
    /// # Return
    ///
    /// If the deregistration was successful, `Ok` is returned. Any calls to
    /// `Reactor::turn` that happen after a successful call to `deregister` will
    /// no longer result in notifications getting sent for this registration.
    ///
    /// `Err` is returned if an error is encountered.
    pub(super) fn deregister<T>(&mut self, io: &T) -> io::Result<()>
    where
        T: Evented,
    {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };
        inner.deregister_source(io)
    }

    pub(super) fn clear_readiness(&self, event: ReadyEvent) {
        self.shared.clear_readiness(event);
    }

    /// Polls for events on the I/O resource's `direction` readiness stream.
    ///
    /// If called with a task context, notify the task when a new event is
    /// received.
    pub(super) fn poll_readiness(
        &self,
        cx: &mut Context<'_>,
        direction: Direction,
    ) -> Poll<io::Result<ReadyEvent>> {
        if self.handle.inner().is_none() {
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "reactor gone")));
        }

        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));
        let ev = ready!(self.shared.poll_readiness(cx, direction));
        coop.made_progress();
        Poll::Ready(Ok(ev))
    }
}

cfg_io_readiness! {
    impl Registration {
        pub(super) async fn readiness(&self, interest: mio::Ready) -> io::Result<ReadyEvent> {
            // TODO: does this need to return a `Result`?
            Ok(self.shared.readiness(interest).await)
        }
    }
}
