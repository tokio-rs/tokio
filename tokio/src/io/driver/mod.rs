pub(crate) mod platform;

mod scheduled_io;
pub(crate) use scheduled_io::ScheduledIo; // pub(crate) for tests

use crate::park::{Park, Unpark};
use crate::runtime::context;
use crate::util::bit;
use crate::util::slab::{self, Slab};

use mio::event::Evented;
use std::fmt;
use std::io;
use std::sync::{Arc, Weak};
use std::task::Waker;
use std::time::Duration;

/// I/O driver, backed by Mio
pub(crate) struct Driver {
    /// Tracks the number of times `turn` is called. It is safe for this to wrap
    /// as it is mostly used to determine when to call `compact()`
    tick: u16,

    /// Reuse the `mio::Events` value across calls to poll.
    events: Option<mio::Events>,

    /// Primary slab handle containing the state for each resource registered
    /// with this driver.
    resources: Slab<ScheduledIo>,

    /// State shared between the reactor and the handles.
    inner: Arc<Inner>,

    _wakeup_registration: mio::Registration,
}

/// A reference to an I/O driver
#[derive(Clone)]
pub(crate) struct Handle {
    inner: Weak<Inner>,
}

pub(super) struct Inner {
    /// The underlying system event queue.
    io: mio::Poll,

    /// Allocates `ScheduledIo` handles when creating new resources.
    pub(super) io_dispatch: slab::Allocator<ScheduledIo>,

    /// Used to wake up the reactor from a call to `turn`
    wakeup: mio::SetReadiness,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub(super) enum Direction {
    Read,
    Write,
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
        let io = mio::Poll::new()?;
        let wakeup_pair = mio::Registration::new2();
        let slab = Slab::new();
        let allocator = slab.allocator();

        io.register(
            &wakeup_pair.0,
            TOKEN_WAKEUP,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )?;

        Ok(Driver {
            tick: 0,
            events: Some(mio::Events::with_capacity(1024)),
            resources: slab,
            _wakeup_registration: wakeup_pair.0,
            inner: Arc::new(Inner {
                io,
                io_dispatch: allocator,
                wakeup: wakeup_pair.1,
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
        const COMPACT_INTERVAL: u16 = 256;

        self.tick = self.tick.wrapping_add(1);

        if self.tick % COMPACT_INTERVAL == 0 {
            self.resources.compact();
        }

        let mut events = self.events.take().expect("i/o driver event store missing");

        // Block waiting for an event to happen, peeling out how many events
        // happened.
        match self.inner.io.poll(&mut events, max_wait) {
            Ok(_) => {}
            Err(e) => return Err(e),
        }

        // Process all the events that came in, dispatching appropriately

        for event in events.iter() {
            let token = event.token();

            if token == TOKEN_WAKEUP {
                self.inner
                    .wakeup
                    .set_readiness(mio::Ready::empty())
                    .unwrap();
            } else {
                self.dispatch(token, event.readiness());
            }
        }

        self.events = Some(events);

        Ok(())
    }

    fn dispatch(&mut self, token: mio::Token, ready: mio::Ready) {
        let mut rd = None;
        let mut wr = None;

        let addr = slab::Address::from_usize(ADDRESS.unpack(token.0));

        let io = match self.resources.get(addr) {
            Some(io) => io,
            None => return,
        };

        if io
            .set_readiness(Some(token.0), |curr| curr | ready.as_usize())
            .is_err()
        {
            // token no longer valid!
            return;
        }

        if ready.is_writable() || platform::is_hup(ready) || platform::is_error(ready) {
            wr = io.writer.take_waker();
        }

        if !(ready & (!mio::Ready::writable())).is_empty() {
            rd = io.reader.take_waker();
        }

        if let Some(w) = rd {
            w.wake();
        }

        if let Some(w) = wr {
            w.wake();
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        self.resources.for_each(|io| {
            // If a task is waiting on the I/O resource, notify it. The task
            // will then attempt to use the I/O resource and fail due to the
            // driver being shutdown.
            io.reader.wake();
            io.writer.wake();
        })
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

impl Handle {
    /// Returns a handle to the current reactor
    ///
    /// # Panics
    ///
    /// This function panics if there is no current reactor set.
    pub(super) fn current() -> Self {
        context::io_handle()
            .expect("there is no reactor running, must be called from the context of Tokio runtime")
    }

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
            inner.wakeup.set_readiness(mio::Ready::readable()).unwrap();
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
        source: &dyn Evented,
        ready: mio::Ready,
    ) -> io::Result<slab::Ref<ScheduledIo>> {
        let (address, shared) = self.io_dispatch.allocate().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "reactor at max registered I/O resources",
            )
        })?;

        let token = GENERATION.pack(shared.generation(), ADDRESS.pack(address.as_usize(), 0));

        self.io
            .register(source, mio::Token(token), ready, mio::PollOpt::edge())?;

        Ok(shared)
    }

    /// Deregisters an I/O resource from the reactor.
    pub(super) fn deregister_source(&self, source: &dyn Evented) -> io::Result<()> {
        self.io.deregister(source)
    }

    /// Registers interest in the I/O resource associated with `token`.
    pub(super) fn register(&self, io: &slab::Ref<ScheduledIo>, dir: Direction, w: Waker) {
        let waker = match dir {
            Direction::Read => &io.reader,
            Direction::Write => &io.writer,
        };

        waker.register(w);
    }
}

impl Direction {
    pub(super) fn mask(self) -> mio::Ready {
        match self {
            Direction::Read => {
                // Everything except writable is signaled through read.
                mio::Ready::all() - mio::Ready::writable()
            }
            Direction::Write => mio::Ready::writable() | platform::hup() | platform::error(),
        }
    }
}
