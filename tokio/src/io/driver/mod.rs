#![cfg_attr(not(feature = "rt"), allow(dead_code))]

mod interest;
#[allow(unreachable_pub)]
pub use interest::Interest;

mod ready;
#[allow(unreachable_pub)]
pub use ready::Ready;

mod registration;
pub(crate) use registration::Registration;

mod scheduled_io;
use scheduled_io::ScheduledIo;

use crate::park::{Park, Unpark};
use crate::util::slab::{self, Slab};
use crate::{loom::sync::Mutex, util::bit};

use std::fmt;
use std::io;
use std::sync::{Arc, Weak};
use std::time::Duration;

/// I/O driver, backed by Mio
pub(crate) struct Driver {
    /// Tracks the number of times `turn` is called. It is safe for this to wrap
    /// as it is mostly used to determine when to call `compact()`
    tick: u8,

    /// Reuse the `mio::Events` value across calls to poll.
    events: Option<mio::Events>,

    /// Primary slab handle containing the state for each resource registered
    /// with this driver. During Drop this is moved into the Inner structure, so
    /// this is an Option to allow it to be vacated (until Drop this is always
    /// Some)
    resources: Option<Slab<ScheduledIo>>,

    /// The system event queue
    poll: mio::Poll,

    /// State shared between the reactor and the handles.
    inner: Arc<Inner>,
}

/// A reference to an I/O driver
#[derive(Clone)]
pub(crate) struct Handle {
    inner: Weak<Inner>,
}

pub(crate) struct ReadyEvent {
    tick: u8,
    pub(crate) ready: Ready,
}

pub(super) struct Inner {
    /// Primary slab handle containing the state for each resource registered
    /// with this driver.
    ///
    /// The ownership of this slab is moved into this structure during
    /// `Driver::drop`, so that `Inner::drop` can notify all outstanding handles
    /// without risking new ones being registered in the meantime.
    resources: Mutex<Option<Slab<ScheduledIo>>>,

    /// Registers I/O resources
    registry: mio::Registry,

    /// Allocates `ScheduledIo` handles when creating new resources.
    pub(super) io_dispatch: slab::Allocator<ScheduledIo>,

    /// Used to wake up the reactor from a call to `turn`
    waker: mio::Waker,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum Direction {
    Read,
    Write,
}

enum Tick {
    Set(u8),
    Clear(u8),
}

// TODO: Don't use a fake token. Instead, reserve a slot entry for the wakeup
// token.
const TOKEN_WAKEUP: mio::Token = mio::Token(1 << 31);

const ADDRESS: bit::Pack = bit::Pack::least_significant(24);

// Packs the generation value in the `readiness` field.
//
// The generation prevents a race condition where a slab slot is reused for a
// new socket while the I/O driver is about to apply a readiness event. The
// generaton value is checked when setting new readiness. If the generation do
// not match, then the readiness event is discarded.
const GENERATION: bit::Pack = ADDRESS.then(7);

fn _assert_kinds() {
    fn _assert<T: Send + Sync>() {}

    _assert::<Handle>();
}

// ===== impl Driver =====

impl Driver {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub(crate) fn new() -> io::Result<Driver> {
        let poll = mio::Poll::new()?;
        let waker = mio::Waker::new(poll.registry(), TOKEN_WAKEUP)?;
        let registry = poll.registry().try_clone()?;

        let slab = Slab::new();
        let allocator = slab.allocator();

        Ok(Driver {
            tick: 0,
            events: Some(mio::Events::with_capacity(1024)),
            poll,
            resources: Some(slab),
            inner: Arc::new(Inner {
                resources: Mutex::new(None),
                registry,
                io_dispatch: allocator,
                waker,
            }),
        })
    }

    /// Returns a handle to this event loop which can be sent across threads
    /// and can be used as a proxy to the event loop itself.
    ///
    /// Handles are cloneable and clones always refer to the same event loop.
    /// This handle is typically passed into functions that create I/O objects
    /// to bind them to this event loop.
    pub(crate) fn handle(&self) -> Handle {
        Handle {
            inner: Arc::downgrade(&self.inner),
        }
    }

    fn turn(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        // How often to call `compact()` on the resource slab
        const COMPACT_INTERVAL: u8 = 255;

        self.tick = self.tick.wrapping_add(1);

        if self.tick == COMPACT_INTERVAL {
            self.resources.as_mut().unwrap().compact()
        }

        let mut events = self.events.take().expect("i/o driver event store missing");

        // Block waiting for an event to happen, peeling out how many events
        // happened.
        match self.poll.poll(&mut events, max_wait) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }

        // Process all the events that came in, dispatching appropriately
        for event in events.iter() {
            let token = event.token();

            if token != TOKEN_WAKEUP {
                self.dispatch(token, Ready::from_mio(event));
            }
        }

        self.events = Some(events);

        Ok(())
    }

    fn dispatch(&mut self, token: mio::Token, ready: Ready) {
        let addr = slab::Address::from_usize(ADDRESS.unpack(token.0));

        let resources = self.resources.as_mut().unwrap();

        let io = match resources.get(addr) {
            Some(io) => io,
            None => return,
        };

        let res = io.set_readiness(Some(token.0), Tick::Set(self.tick), |curr| curr | ready);

        if res.is_err() {
            // token no longer valid!
            return;
        }

        io.wake(ready);
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        (*self.inner.resources.lock()) = self.resources.take();
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let resources = self.resources.lock().take();

        if let Some(mut slab) = resources {
            slab.for_each(|io| {
                // If a task is waiting on the I/O resource, notify it. The task
                // will then attempt to use the I/O resource and fail due to the
                // driver being shutdown.
                io.shutdown();
            });
        }
    }
}

impl Park for Driver {
    type Unpark = Handle;
    type Error = io::Error;

    fn unpark(&self) -> Self::Unpark {
        self.handle()
    }

    fn park(&mut self) -> io::Result<()> {
        self.turn(None)?;
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> io::Result<()> {
        self.turn(Some(duration))?;
        Ok(())
    }

    fn shutdown(&mut self) {}
}

impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Driver")
    }
}

// ===== impl Handle =====

cfg_rt! {
    impl Handle {
        /// Returns a handle to the current reactor
        ///
        /// # Panics
        ///
        /// This function panics if there is no current reactor set and `rt` feature
        /// flag is not enabled.
        pub(super) fn current() -> Self {
            crate::runtime::context::io_handle().expect("A Tokio 1.x context was found, but IO is disabled. Call `enable_io` on the runtime builder to enable IO.")
        }
    }
}

cfg_not_rt! {
    impl Handle {
        /// Returns a handle to the current reactor
        ///
        /// # Panics
        ///
        /// This function panics if there is no current reactor set, or if the `rt`
        /// feature flag is not enabled.
        pub(super) fn current() -> Self {
            panic!("{}", crate::util::error::CONTEXT_MISSING_ERROR)
        }
    }
}

impl Handle {
    /// Forces a reactor blocked in a call to `turn` to wakeup, or otherwise
    /// makes the next call to `turn` return immediately.
    ///
    /// This method is intended to be used in situations where a notification
    /// needs to otherwise be sent to the main reactor. If the reactor is
    /// currently blocked inside of `turn` then it will wake up and soon return
    /// after this method has been called. If the reactor is not currently
    /// blocked in `turn`, then the next call to `turn` will not block and
    /// return immediately.
    fn wakeup(&self) {
        if let Some(inner) = self.inner() {
            inner.waker.wake().expect("failed to wake I/O driver");
        }
    }

    pub(super) fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

impl Unpark for Handle {
    fn unpark(&self) {
        self.wakeup();
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}

// ===== impl Inner =====

impl Inner {
    /// Registers an I/O resource with the reactor for a given `mio::Ready` state.
    ///
    /// The registration token is returned.
    pub(super) fn add_source(
        &self,
        source: &mut impl mio::event::Source,
        interest: Interest,
    ) -> io::Result<slab::Ref<ScheduledIo>> {
        let (address, shared) = self.io_dispatch.allocate().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "reactor at max registered I/O resources",
            )
        })?;

        let token = GENERATION.pack(shared.generation(), ADDRESS.pack(address.as_usize(), 0));

        self.registry
            .register(source, mio::Token(token), interest.to_mio())?;

        Ok(shared)
    }

    /// Deregisters an I/O resource from the reactor.
    pub(super) fn deregister_source(&self, source: &mut impl mio::event::Source) -> io::Result<()> {
        self.registry.deregister(source)
    }
}

impl Direction {
    pub(super) fn mask(self) -> Ready {
        match self {
            Direction::Read => Ready::READABLE | Ready::READ_CLOSED,
            Direction::Write => Ready::WRITABLE | Ready::WRITE_CLOSED,
        }
    }
}
