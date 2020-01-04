//! I/O Driver backed by Mio
//#![allow(unused_imports, dead_code, unreachable_pub)]
use super::platform;
use super::ScheduledIo;
use crate::loom::sync::atomic::AtomicUsize;
use crate::park::{Park, Unpark};
use crate::util::slab::{Address, Slab};
use mio::event::Evented;
use std::fmt;
use std::io;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Weak};
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

// ========= Inner Impl ========== //
struct Inner {
    /// The underlying system event queue.
    io: mio::Poll,

    /// Dispatch slabs for I/O and futures events
    io_dispatch: Slab<ScheduledIo>,

    /// The number of sources in `io_dispatch`.
    n_sources: AtomicUsize,

    /// Used to wake up the I/O driver from a call to `turn`
    wakeup: mio::SetReadiness,
}

impl Inner {
    /// Register an I/O resource with the I/O driver.
    ///
    /// The registration token is returned.
    fn add_source(&self, source: &dyn Evented) -> io::Result<Address> {
        let address = self.io_dispatch.alloc().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "I/O driver at max registered I/O resources",
            )
        })?;
        self.n_sources.fetch_add(1, SeqCst);
        self.io.register(
            source,
            mio::Token(address.to_usize()),
            mio::Ready::all(),
            mio::PollOpt::edge(),
        )?;
        Ok(address)
    }
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Inner")
    }
}

// ========= Driver Impl ========= //

const TOKEN_WAKEUP: mio::Token = mio::Token(Address::NULL);

/// I/O driver backed by Mio. Progress is made via `Driver::turn`.
pub(crate) struct Driver {
    /// Reuse the `mio::Events` value across calls to poll.
    events: mio::Events,
    /// State shared between the I/O driver and the handles.
    inner: Arc<Inner>,
    /// Registration which can be used to wakeup the I/O driver.
    _wakeup_registration: mio::Registration,
}

impl Driver {
    pub(crate) fn new() -> io::Result<Driver> {
        let io = mio::Poll::new()?;
        let (reg, setready) = mio::Registration::new2();
        io.register(
            &reg,
            TOKEN_WAKEUP,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )?;
        let inner = Inner {
            io,
            io_dispatch: Slab::new(),
            n_sources: AtomicUsize::new(0),
            wakeup: setready,
        };
        let inner = Arc::new(inner);
        Ok(Driver {
            events: mio::Events::with_capacity(1024),
            inner,
            _wakeup_registration: reg,
        })
    }

    /// Block on rediness events for any of the `Evented` handles which have been
    /// registered with this driver. This should be called via `Park`.
    fn turn(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        // Block waiting for an event to happen.
        self.inner.io.poll(&mut self.events, max_wait)?;
        // Process each event.
        for event in self.events.iter() {
            let token = event.token();
            // Check to see if there was a wakeup signal for the driver itself.
            if token == TOKEN_WAKEUP {
                self.inner
                    .wakeup
                    .set_readiness(mio::Ready::empty())
                    .unwrap();
            } else {
                self.dispatch(token, event.readiness())
            }
        }
        Ok(())
    }

    /// Dispatch I/O events for any of the `Evented` handles which have been
    /// registered with this driver and have signaled readiness.
    fn dispatch(&self, token: mio::Token, ready: mio::Ready) {
        let mut read = None;
        let mut write = None;
        let address = Address::from_usize(token.0);
        if let Some(io) = self.inner.io_dispatch.get(address) {
            if let Err(_) = io.set_readiness(address, |curr| curr | ready.as_usize()) {
                // token is no longer ready
                return;
            }

            if ready.is_writable() || platform::is_hup(ready) {
                write = io.writer.take_waker();
            }

            if !(ready & (!mio::Ready::writable())).is_empty() {
                read = io.reader.take_waker();
            }

            if let Some(w) = read {
                w.wake();
            }

            if let Some(w) = write {
                w.wake();
            }
        }
    }

    pub(crate) fn handle(&self) -> Handle {
        let inner = Arc::downgrade(&self.inner);
        Handle { inner }
    }
}

/// Used to wake the I/O driver after `Park`.
#[derive(Clone)]
pub(crate) struct DriverUnpark {
    inner: Weak<Inner>,
}

impl Unpark for DriverUnpark {
    fn unpark(&self) {
        if let Some(inner) = self.inner.upgrade() {
            inner.wakeup.set_readiness(mio::Ready::readable()).unwrap();
        }
    }
}

impl Park for Driver {
    type Unpark = DriverUnpark;
    type Error = io::Error;
    fn unpark(&self) -> Self::Unpark {
        let inner = Arc::downgrade(&self.inner);
        DriverUnpark { inner }
    }
    fn park(&mut self) -> Result<(), Self::Error> {
        self.turn(None)
    }
    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.turn(Some(duration))
    }
}

impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Driver")
    }
}
// ========== Handle Impl ========== //

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum Direction {
    Read,
    Write,
}

impl Direction {
    fn mask(self) -> mio::Ready {
        match self {
            Direction::Read => {
                // Everything except writable is signaled through read.
                mio::Ready::all() - mio::Ready::writable()
            }
            Direction::Write => mio::Ready::writable() | platform::hup(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Handle {
    inner: Weak<Inner>,
}

impl Handle {
    fn inner(&self) -> io::Result<Arc<Inner>> {
        self.inner.upgrade().ok_or(io::Error::new(
            io::ErrorKind::Other,
            "I/O driver not present",
        ))
    }

    /// Deregisters an I/O resource from the I/O driver.
    fn deregister_source(&self, source: &dyn Evented) -> io::Result<()> {
        let inner = self.inner()?;
        inner.io.deregister(source)
    }

    /// Attempt to remove the source from the set of dispatchable registrations if
    /// the I/O driver is still present.
    fn drop_source(&self, address: Address) {
        if let Ok(inner) = self.inner() {
            inner.io_dispatch.remove(address);
            inner.n_sources.fetch_sub(1, SeqCst);
        }
    }
    /// Registers interest in the I/O resource associated with `token`.
    fn register(&self, token: Address, dir: Direction, w: Waker) -> io::Result<()> {
        let inner = self.inner()?;
        let scheduled = inner
            .io_dispatch
            .get(token)
            .unwrap_or_else(|| panic!("IO resource for token {:?} does not exist!", token));

        let readiness = scheduled
            .get_readiness(token)
            .unwrap_or_else(|| panic!("token {:?} no longer valid!", token));
        let (waker, ready) = match dir {
            Direction::Read => (&scheduled.reader, !mio::Ready::writable()),
            Direction::Write => (&scheduled.writer, mio::Ready::writable()),
        };

        waker.register(w);

        if readiness & ready.as_usize() != 0 {
            waker.wake();
        }
        Ok(())
    }

    /// Register the I/O resource with the default I/O driver.
    ///
    /// # Return
    ///
    /// - `Ok` if the registration happened successfully
    /// - `Err` if an error was encountered during registration
    pub(crate) fn register_io<T>(&self, io: &T) -> io::Result<Registration>
    where
        T: Evented,
    {
        let inner = self.inner()?;
        let address = inner.add_source(io)?;
        Ok(Registration {
            handle: Handle {
                inner: Arc::downgrade(&inner),
            },
            address,
        })
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}

// ========= impl Registration ======== //

/// Associates an I/O resource with the I/O driver instance that drives it.
///
/// A registration represents an I/O resource registered with a I/O driver such
/// that it will receive task notifications on readiness. This is the lowest
/// level API for integrating with a I/O driver.
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
/// [`new`]: #method.new
/// [`poll_read_ready`]: #method.poll_read_ready`]
/// [`poll_write_ready`]: #method.poll_write_ready`]

#[derive(Debug)]
pub struct Registration {
    handle: Handle,
    address: Address,
}

impl Registration {
    /// Deregister the I/O resource from the I/O driver it is associated with.
    ///
    /// This function must be called before the I/O resource associated with the
    /// registration is dropped.
    ///
    /// Note that deregistering does not guarantee that the I/O resource can be
    /// registered with a different I/O driver. Some I/O resource types can only be
    /// associated with a single I/O driver instance for their lifetime.
    ///
    /// # Return
    ///
    /// If the deregistration was successful, `Ok` is returned. Any calls to
    /// `I/O driver::turn` that happen after a successful call to `deregister` will
    /// no longer result in notifications getting sent for this registration.
    ///
    /// `Err` is returned if an error is encountered.
    pub fn deregister<T>(&mut self, io: &T) -> io::Result<()>
    where
        T: Evented,
    {
        self.handle.deregister_source(io)
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
        let inner = self.handle.inner()?;
        // If the task should be notified about new events, ensure that it has
        // been registered
        if let Some(ref cx) = cx {
            self.handle
                .register(self.address, direction, cx.waker().clone())?;
        }

        let mask = direction.mask();
        let mask_no_hup = (mask - platform::hup()).as_usize();

        let sched = inner.io_dispatch.get(self.address).unwrap();

        // This consumes the current readiness state **except** for HUP. HUP is
        // excluded because a) it is a final state and never transitions out of
        // HUP and b) both the read AND the write directions need to be able to
        // observe this state.
        //
        // If HUP were to be cleared when `direction` is `Read`, then when
        // `poll_ready` is called again with a _`direction` of `Write`, the HUP
        // state would not be visible.
        let curr_ready = sched
            .set_readiness(self.address, |curr| curr & (!mask_no_hup))
            .unwrap_or_else(|_| panic!("address {:?} no longer valid!", self.address));

        let mut ready = mask & mio::Ready::from_usize(curr_ready);

        if ready.is_empty() {
            if let Some(cx) = cx {
                // Update the task info
                match direction {
                    Direction::Read => sched.reader.register_by_ref(cx.waker()),
                    Direction::Write => sched.writer.register_by_ref(cx.waker()),
                }

                // Try again
                let curr_ready = sched
                    .set_readiness(self.address, |curr| curr & (!mask_no_hup))
                    .unwrap_or_else(|_| panic!("address {:?} no longer valid!", self.address));
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

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

impl Drop for Registration {
    fn drop(&mut self) {
        self.handle.drop_source(self.address)
    }
}

#[cfg(all(test, loom))]
mod tests {
    use super::*;
    use loom::thread;

    // No-op `Evented` impl just so we can have something to pass to `add_source`.
    struct NotEvented;

    impl Evented for NotEvented {
        fn register(
            &self,
            _: &mio::Poll,
            _: mio::Token,
            _: mio::Ready,
            _: mio::PollOpt,
        ) -> io::Result<()> {
            Ok(())
        }

        fn reregister(
            &self,
            _: &mio::Poll,
            _: mio::Token,
            _: mio::Ready,
            _: mio::PollOpt,
        ) -> io::Result<()> {
            Ok(())
        }

        fn deregister(&self, _: &mio::Poll) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn tokens_unique_when_dropped() {
        loom::model(|| {
            let reactor = Driver::new().unwrap();
            let inner = reactor.inner;
            let inner2 = inner.clone();

            let token_1 = inner.add_source(&NotEvented).unwrap();
            let thread = thread::spawn(move || {
                inner2.drop_source(token_1);
            });

            let token_2 = inner.add_source(&NotEvented).unwrap();
            thread.join().unwrap();

            assert!(token_1 != token_2);
        })
    }

    #[test]
    fn tokens_unique_when_dropped_on_full_page() {
        loom::model(|| {
            let reactor = Driver::new().unwrap();
            let inner = reactor.inner;
            let inner2 = inner.clone();
            // add sources to fill up the first page so that the dropped index
            // may be reused.
            for _ in 0..31 {
                inner.add_source(&NotEvented).unwrap();
            }

            let token_1 = inner.add_source(&NotEvented).unwrap();
            let thread = thread::spawn(move || {
                inner2.drop_source(token_1);
            });

            let token_2 = inner.add_source(&NotEvented).unwrap();
            thread.join().unwrap();

            assert!(token_1 != token_2);
        })
    }

    #[test]
    fn tokens_unique_concurrent_add() {
        loom::model(|| {
            let reactor = Driver::new().unwrap();
            let inner = reactor.inner;
            let inner2 = inner.clone();

            let thread = thread::spawn(move || {
                let token_2 = inner2.add_source(&NotEvented).unwrap();
                token_2
            });

            let token_1 = inner.add_source(&NotEvented).unwrap();
            let token_2 = thread.join().unwrap();

            assert!(token_1 != token_2);
        })
    }
}
