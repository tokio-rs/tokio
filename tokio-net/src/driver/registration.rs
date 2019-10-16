use super::platform;
use super::reactor::{Direction, Handle, HandlePriv};

use mio::{self, Evented};
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll, Waker};
use std::{io, usize};

/// Associates an I/O resource with the reactor instance that drives it.
///
/// A registration represents an I/O resource registered with a Reactor such
/// that it will receive task notifications on readiness. This is the lowest
/// level API for integrating with a reactor.
///
/// The association between an I/O resource is made by calling [`register`].
/// Once the association is established, it remains established until the
/// registration instance is dropped. Subsequent calls to [`register`] are
/// no-ops.
///
/// A registration instance represents two separate readiness streams. One for
/// the read readiness and one for write readiness. These streams are
/// independent and can be consumed from separate tasks.
///
/// **Note**: while `Registration` is `Sync`, the caller must ensure that there
/// are at most two tasks that use a registration instance concurrently. One
/// task for [`poll_read_ready`] and one task for [`poll_write_ready`]. While
/// violating this requirement is "safe" from a Rust memory safety point of
/// view, it will result in unexpected behavior in the form of lost
/// notifications and tasks hanging.
///
/// ## Platform-specific events
///
/// `Registration` also allows receiving platform-specific `mio::Ready` events.
/// These events are included as part of the read readiness event stream. The
/// write readiness event stream is only for `Ready::writable()` events.
///
/// [`register`]: #method.register
/// [`poll_read_ready`]: #method.poll_read_ready`]
/// [`poll_write_ready`]: #method.poll_write_ready`]
#[derive(Debug, Default)]
pub struct Registration {
    inner: Option<Inner>,
}

#[derive(Debug)]
struct Inner {
    handle: HandlePriv,
    token: usize,
}

// ===== impl Registration =====

impl Registration {
    /// Register the I/O resource with the default reactor.
    ///
    /// This function is safe to call concurrently and repeatedly. However, only
    /// the first call will establish the registration. Subsequent calls will be
    /// no-ops.
    ///
    /// # Return
    ///
    /// If the registration happened successfully, `Ok` is returned.
    ///
    /// If an error is encountered during registration, `Err` is returned.
    pub fn register<T>(&mut self, io: &T) -> io::Result<()>
    where
        T: Evented,
    {
        self.register2(io, HandlePriv::try_current)
    }

    /// Register the I/O resource with the specified reactor.
    ///
    /// This function is safe to call concurrently and repeatedly. However, only
    /// the first call will establish the registration. Subsequent calls will be
    /// no-ops.
    ///
    /// If the registration happened successfully, `Ok` is returned.
    ///
    /// If an error is encountered during registration, `Err` is returned.
    pub fn register_with<T>(&mut self, io: &T, handle: &Handle) -> io::Result<()>
    where
        T: Evented,
    {
        self.register2(io, || match handle.as_priv() {
            Some(handle) => Ok(handle.clone()),
            None => HandlePriv::try_current(),
        })
    }

    pub(crate) fn register_with_priv<T>(&mut self, io: &T, handle: &HandlePriv) -> io::Result<()>
    where
        T: Evented,
    {
        self.register2(io, || Ok(handle.clone()))
    }

    /// Deregister the I/O resource from the reactor it is associated with.
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
        if let Some(inner) = self.inner.as_ref() {
            inner.deregister(io)?
        }
        Ok(())
    }

    /// Register an I/O resource with a reactor.
    ///
    /// Once the association is established, it remains established until the
    /// registration instance is dropped.
    fn register2<T, F>(&mut self, io: &T, f: F) -> io::Result<()>
    where
        T: Evented,
        F: Fn() -> io::Result<HandlePriv>,
    {
        if self.inner.is_none() {
            let handle = f()?;
            let inner = Inner::add_source(io, handle)?;
            self.inner = Some(inner);
        }
        Ok(())
    }

    /// Poll for events on the I/O resource's read readiness stream.
    ///
    /// If the I/O resource receives a new read readiness event since the last
    /// call to `poll_read_ready`, it is returned. If it has not, the current
    /// task is notified once a new event is received.
    ///
    /// All events except `HUP` are [edge-triggered]. Once `HUP` is returned,
    /// the function will always return `Ready(HUP)`. This should be treated as
    /// the end of the readiness stream.
    ///
    /// Ensure that [`register`] has been called first.
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
    ///   error. This error either represents a permanent internal error **or**
    ///   the fact that [`register`] was not called first.
    ///
    /// [`register`]: #method.register
    /// [edge-triggered]: https://docs.rs/mio/0.6/mio/struct.Poll.html#edge-triggered-and-level-triggered
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        let v = self.poll_ready(Direction::Read, Some(cx))?;
        match v {
            Some(v) => Poll::Ready(Ok(v)),
            None => Poll::Pending,
        }
    }

    /// Consume any pending read readiness event.
    ///
    /// This function is identical to [`poll_read_ready`] **except** that it
    /// will not notify the current task when a new event is received. As such,
    /// it is safe to call this function from outside of a task context.
    ///
    /// [`poll_read_ready`]: #method.poll_read_ready
    pub fn take_read_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Read, None)
    }

    /// Poll for events on the I/O resource's write readiness stream.
    ///
    /// If the I/O resource receives a new write readiness event since the last
    /// call to `poll_write_ready`, it is returned. If it has not, the current
    /// task is notified once a new event is received.
    ///
    /// All events except `HUP` are [edge-triggered]. Once `HUP` is returned,
    /// the function will always return `Ready(HUP)`. This should be treated as
    /// the end of the readiness stream.
    ///
    /// Ensure that [`register`] has been called first.
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
    ///   error. This error either represents a permanent internal error **or**
    ///   the fact that [`register`] was not called first.
    ///
    /// [`register`]: #method.register
    /// [edge-triggered]: https://docs.rs/mio/0.6/mio/struct.Poll.html#edge-triggered-and-level-triggered
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        let v = self.poll_ready(Direction::Write, Some(cx))?;
        match v {
            Some(v) => Poll::Ready(Ok(v)),
            None => Poll::Pending,
        }
    }

    /// Consume any pending write readiness event.
    ///
    /// This function is identical to [`poll_write_ready`] **except** that it
    /// will not notify the current task when a new event is received. As such,
    /// it is safe to call this function from outside of a task context.
    ///
    /// [`poll_write_ready`]: #method.poll_write_ready
    pub fn take_write_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Write, None)
    }

    /// Poll for events on the I/O resource's `direction` readiness stream.
    ///
    /// If called with a task context, notify the task when a new event is
    /// received.
    fn poll_ready(
        &self,
        direction: Direction,
        cx: Option<&mut Context<'_>>,
    ) -> io::Result<Option<mio::Ready>> {
        let inner = self.inner.as_ref().ok_or(io::Error::new(
            io::ErrorKind::Other,
            "I/O resource has not been registered to a reactor",
        ))?;
        if let Some(ref cx) = cx {
            inner.register(direction, cx.waker().clone());
        }
        inner.poll_ready(direction, cx)
    }
}

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

// ===== impl Inner =====

impl Inner {
    fn add_source<E>(io: &E, handle: HandlePriv) -> io::Result<Self>
    where
        E: Evented,
    {
        let token = if let Some(inner) = handle.inner() {
            inner.add_source(io)?
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to find event loop",
            ));
        };
        Ok(Self { handle, token })
    }

    fn register(&self, direction: Direction, waker: Waker) {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => {
                waker.wake();
                return;
            }
        };
        inner.register(self.token, direction, waker);
    }

    fn deregister<E: Evented>(&self, io: &E) -> io::Result<()> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };
        inner.deregister_source(io)
    }

    fn poll_ready(
        &self,
        direction: Direction,
        cx: Option<&mut Context<'_>>,
    ) -> io::Result<Option<mio::Ready>> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        let mask = direction.mask();
        let mask_no_hup = (mask - platform::hup()).as_usize();

        let io_dispatch = inner.io_dispatch.read();
        let sched = &io_dispatch[self.token];

        // This consumes the current readiness state **except** for HUP. HUP is
        // excluded because a) it is a final state and never transitions out of
        // HUP and b) both the read AND the write directions need to be able to
        // observe this state.
        //
        // If HUP were to be cleared when `direction` is `Read`, then when
        // `poll_ready` is called again with a _`direction` of `Write`, the HUP
        // state would not be visible.
        let mut ready =
            mask & mio::Ready::from_usize(sched.readiness.fetch_and(!mask_no_hup, SeqCst));

        if ready.is_empty() {
            if let Some(cx) = cx {
                debug!(message = "scheduling", ?direction, token = self.token);
                // Update the task info
                match direction {
                    Direction::Read => sched.reader.register_by_ref(cx.waker()),
                    Direction::Write => sched.writer.register_by_ref(cx.waker()),
                }

                // Try again
                ready =
                    mask & mio::Ready::from_usize(sched.readiness.fetch_and(!mask_no_hup, SeqCst));
            }
        }

        if ready.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ready))
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return,
        };
        inner.drop_source(self.token);
    }
}
