use crate::io::driver::{platform, Direction, Handle, ScheduledIo};
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
    pub struct Registration {
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
    /// Registers the I/O resource with the default reactor.
    ///
    /// # Return
    ///
    /// - `Ok` if the registration happened successfully
    /// - `Err` if an error was encountered during registration
    ///
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn new<T>(io: &T) -> io::Result<Registration>
    where
        T: Evented,
    {
        Registration::new_with_ready(io, mio::Ready::all())
    }

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
    ///
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn new_with_ready<T>(io: &T, ready: mio::Ready) -> io::Result<Registration>
    where
        T: Evented,
    {
        let handle = Handle::current();
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
    pub fn deregister<T>(&mut self, io: &T) -> io::Result<()>
    where
        T: Evented,
    {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };
        inner.deregister_source(io)
    }

    /// Polls for events on the I/O resource's read readiness stream.
    ///
    /// If the I/O resource receives a new read readiness event since the last
    /// call to `poll_read_ready`, it is returned. If it has not, the current
    /// task is notified once a new event is received.
    ///
    /// All events except `HUP` are [edge-triggered]. Once `HUP` is returned,
    /// the function will always return `Ready(HUP)`. This should be treated as
    /// the end of the readiness stream.
    ///
    /// # Return value
    ///
    /// There are several possible return values:
    ///
    /// * `Poll::Ready(Ok(readiness))` means that the I/O resource has received
    ///   a new readiness event. The readiness value is included.
    ///
    /// * `Poll::Pending` means that no new readiness events have been received
    ///   since the last call to `poll_read_ready`.
    ///
    /// * `Poll::Ready(Err(err))` means that the registration has encountered an
    ///   error. This could represent a permanent internal error for example.
    ///
    /// [edge-triggered]: struct@mio::Poll#edge-triggered-and-level-triggered
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        let v = self.poll_ready(Direction::Read, Some(cx)).map_err(|e| {
            coop.made_progress();
            e
        })?;
        match v {
            Some(v) => {
                coop.made_progress();
                Poll::Ready(Ok(v))
            }
            None => Poll::Pending,
        }
    }

    /// Consume any pending read readiness event.
    ///
    /// This function is identical to [`poll_read_ready`] **except** that it
    /// will not notify the current task when a new event is received. As such,
    /// it is safe to call this function from outside of a task context.
    ///
    /// [`poll_read_ready`]: method@Self::poll_read_ready
    pub fn take_read_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Read, None)
    }

    /// Polls for events on the I/O resource's write readiness stream.
    ///
    /// If the I/O resource receives a new write readiness event since the last
    /// call to `poll_write_ready`, it is returned. If it has not, the current
    /// task is notified once a new event is received.
    ///
    /// All events except `HUP` are [edge-triggered]. Once `HUP` is returned,
    /// the function will always return `Ready(HUP)`. This should be treated as
    /// the end of the readiness stream.
    ///
    /// # Return value
    ///
    /// There are several possible return values:
    ///
    /// * `Poll::Ready(Ok(readiness))` means that the I/O resource has received
    ///   a new readiness event. The readiness value is included.
    ///
    /// * `Poll::Pending` means that no new readiness events have been received
    ///   since the last call to `poll_write_ready`.
    ///
    /// * `Poll::Ready(Err(err))` means that the registration has encountered an
    ///   error. This could represent a permanent internal error for example.
    ///
    /// [edge-triggered]: struct@mio::Poll#edge-triggered-and-level-triggered
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        let v = self.poll_ready(Direction::Write, Some(cx)).map_err(|e| {
            coop.made_progress();
            e
        })?;
        match v {
            Some(v) => {
                coop.made_progress();
                Poll::Ready(Ok(v))
            }
            None => Poll::Pending,
        }
    }

    /// Consumes any pending write readiness event.
    ///
    /// This function is identical to [`poll_write_ready`] **except** that it
    /// will not notify the current task when a new event is received. As such,
    /// it is safe to call this function from outside of a task context.
    ///
    /// [`poll_write_ready`]: method@Self::poll_write_ready
    pub fn take_write_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Write, None)
    }

    /// Polls for events on the I/O resource's `direction` readiness stream.
    ///
    /// If called with a task context, notify the task when a new event is
    /// received.
    fn poll_ready(
        &self,
        direction: Direction,
        cx: Option<&mut Context<'_>>,
    ) -> io::Result<Option<mio::Ready>> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        // If the task should be notified about new events, ensure that it has
        // been registered
        if let Some(ref cx) = cx {
            inner.register(&self.shared, direction, cx.waker().clone())
        }

        let mask = direction.mask();
        let mask_no_hup = (mask - platform::hup() - platform::error()).as_usize();

        // This consumes the current readiness state **except** for HUP and
        // error. HUP and error are excluded because a) they are final states
        // and never transitition out and b) both the read AND the write
        // directions need to be able to obvserve these states.
        //
        // # Platform-specific behavior
        //
        // HUP and error readiness are platform-specific. On epoll platforms,
        // HUP has specific conditions that must be met by both peers of a
        // connection in order to be triggered.
        //
        // On epoll platforms, `EPOLLERR` is signaled through
        // `UnixReady::error()` and is important to be observable by both read
        // AND write. A specific case that `EPOLLERR` occurs is when the read
        // end of a pipe is closed. When this occurs, a peer blocked by
        // writing to the pipe should be notified.
        let curr_ready = self
            .shared
            .set_readiness(None, |curr| curr & (!mask_no_hup))
            .unwrap_or_else(|_| unreachable!());

        let mut ready = mask & mio::Ready::from_usize(curr_ready);

        if ready.is_empty() {
            if let Some(cx) = cx {
                // Update the task info
                match direction {
                    Direction::Read => self.shared.reader.register_by_ref(cx.waker()),
                    Direction::Write => self.shared.writer.register_by_ref(cx.waker()),
                }

                // Try again
                let curr_ready = self
                    .shared
                    .set_readiness(None, |curr| curr & (!mask_no_hup))
                    .unwrap();
                ready = mask & mio::Ready::from_usize(curr_ready);
            }
        }

        if ready.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ready))
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        drop(self.shared.reader.take_waker());
        drop(self.shared.writer.take_waker());
    }
}
