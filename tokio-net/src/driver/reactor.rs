use super::platform;
use super::sharded_rwlock::RwLock;

use tokio_executor::park::{Park, Unpark};
use tokio_sync::AtomicWaker;

use mio::event::Evented;
use slab::Slab;
use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
#[cfg(all(unix, not(target_os = "fuchsia")))]
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::sync::{Arc, Weak};
use std::task::Waker;
use std::time::Duration;
use std::{fmt, usize};

/// The core reactor, or event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
pub struct Reactor {
    /// Reuse the `mio::Events` value across calls to poll.
    events: mio::Events,

    /// State shared between the reactor and the handles.
    inner: Arc<Inner>,

    _wakeup_registration: mio::Registration,
}

/// A reference to a reactor.
///
/// A `Handle` is used for associating I/O objects with an event loop
/// explicitly. Typically though you won't end up using a `Handle` that often
/// and will instead use the default reactor for the execution context.
///
/// By default, most components bind lazily to reactors.
/// To get this behavior when manually passing a `Handle`, use `default()`.
#[derive(Clone)]
pub struct Handle {
    inner: Option<HandlePriv>,
}

/// Like `Handle`, but never `None`.
#[derive(Clone)]
pub(crate) struct HandlePriv {
    inner: Weak<Inner>,
}

/// Return value from the `turn` method on `Reactor`.
///
/// Currently this value doesn't actually provide any functionality, but it may
/// in the future give insight into what happened during `turn`.
#[derive(Debug)]
pub struct Turn {
    _priv: (),
}

#[test]
fn test_handle_size() {
    use std::mem;
    assert_eq!(mem::size_of::<Handle>(), mem::size_of::<HandlePriv>());
}

pub(super) struct Inner {
    /// The underlying system event queue.
    io: mio::Poll,

    /// ABA guard counter
    next_aba_guard: AtomicUsize,

    /// Dispatch slabs for I/O and futures events
    pub(super) io_dispatch: RwLock<Slab<ScheduledIo>>,

    /// Used to wake up the reactor from a call to `turn`
    wakeup: mio::SetReadiness,
}

pub(super) struct ScheduledIo {
    aba_guard: usize,
    pub(super) readiness: AtomicUsize,
    pub(super) reader: AtomicWaker,
    pub(super) writer: AtomicWaker,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub(super) enum Direction {
    Read,
    Write,
}

thread_local! {
    /// Tracks the reactor for the current execution context.
    static CURRENT_REACTOR: RefCell<Option<HandlePriv>> = RefCell::new(None)
}

const TOKEN_SHIFT: usize = 22;

// Kind of arbitrary, but this reserves some token space for later usage.
const MAX_SOURCES: usize = (1 << TOKEN_SHIFT) - 1;
const TOKEN_WAKEUP: mio::Token = mio::Token(MAX_SOURCES);

fn _assert_kinds() {
    fn _assert<T: Send + Sync>() {}

    _assert::<Handle>();
}

// ===== impl Reactor =====

#[derive(Debug)]
///Guard that resets current reactor on drop.
pub struct DefaultGuard<'a> {
    _lifetime: PhantomData<&'a u8>,
}

impl Drop for DefaultGuard<'_> {
    fn drop(&mut self) {
        CURRENT_REACTOR.with(|current| {
            let mut current = current.borrow_mut();
            *current = None;
        });
    }
}

///Sets handle for a default reactor, returning guard that unsets it on drop.
pub fn set_default(handle: &Handle) -> DefaultGuard<'_> {
    CURRENT_REACTOR.with(|current| {
        let mut current = current.borrow_mut();

        assert!(
            current.is_none(),
            "default Tokio reactor already set \
             for execution context"
        );

        let handle = match handle.as_priv() {
            Some(handle) => handle,
            None => {
                panic!("`handle` does not reference a reactor");
            }
        };

        *current = Some(handle.clone());
    });

    DefaultGuard {
        _lifetime: PhantomData,
    }
}

impl Reactor {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Reactor> {
        let io = mio::Poll::new()?;
        let wakeup_pair = mio::Registration::new2();

        io.register(
            &wakeup_pair.0,
            TOKEN_WAKEUP,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )?;

        Ok(Reactor {
            events: mio::Events::with_capacity(1024),
            _wakeup_registration: wakeup_pair.0,
            inner: Arc::new(Inner {
                io,
                next_aba_guard: AtomicUsize::new(0),
                io_dispatch: RwLock::new(Slab::with_capacity(1)),
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
    pub fn handle(&self) -> Handle {
        Handle {
            inner: Some(HandlePriv {
                inner: Arc::downgrade(&self.inner),
            }),
        }
    }

    /// Performs one iteration of the event loop, blocking on waiting for events
    /// for at most `max_wait` (forever if `None`).
    ///
    /// This method is the primary method of running this reactor and processing
    /// I/O events that occur. This method executes one iteration of an event
    /// loop, blocking at most once waiting for events to happen.
    ///
    /// If a `max_wait` is specified then the method should block no longer than
    /// the duration specified, but this shouldn't be used as a super-precise
    /// timer but rather a "ballpark approximation"
    ///
    /// # Return value
    ///
    /// This function returns an instance of `Turn`
    ///
    /// `Turn` as of today has no extra information with it and can be safely
    /// discarded.  In the future `Turn` may contain information about what
    /// happened while this reactor blocked.
    ///
    /// # Errors
    ///
    /// This function may also return any I/O error which occurs when polling
    /// for readiness of I/O objects with the OS. This is quite unlikely to
    /// arise and typically mean that things have gone horribly wrong at that
    /// point. Currently this is primarily only known to happen for internal
    /// bugs to `tokio` itself.
    pub fn turn(&mut self, max_wait: Option<Duration>) -> io::Result<Turn> {
        self.poll(max_wait)?;
        Ok(Turn { _priv: () })
    }

    /// Returns true if the reactor is currently idle.
    ///
    /// Idle is defined as all tasks that have been spawned have completed,
    /// either successfully or with an error.
    pub fn is_idle(&self) -> bool {
        self.inner.io_dispatch.read().is_empty()
    }

    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug"))]
    fn poll(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        // Block waiting for an event to happen, peeling out how many events
        // happened.
        match self.inner.io.poll(&mut self.events, max_wait) {
            Ok(_) => {}
            Err(e) => return Err(e),
        }

        // Process all the events that came in, dispatching appropriately

        // event count is only used for  tracing instrumentation.
        #[cfg(feature = "tracing")]
        let mut events = 0;

        for event in self.events.iter() {
            #[cfg(feature = "tracing")]
            {
                events += 1;
            }
            let token = event.token();
            trace!(event.readiness = ?event.readiness(), event.token = ?token);

            if token == TOKEN_WAKEUP {
                self.inner
                    .wakeup
                    .set_readiness(mio::Ready::empty())
                    .unwrap();
            } else {
                self.dispatch(token, event.readiness());
            }
        }

        trace!(message = "loop process", events);

        Ok(())
    }

    fn dispatch(&self, token: mio::Token, ready: mio::Ready) {
        let aba_guard = token.0 & !MAX_SOURCES;
        let token = token.0 & MAX_SOURCES;

        let mut rd = None;
        let mut wr = None;

        // Create a scope to ensure that notifying the tasks stays out of the
        // lock's critical section.
        {
            let io_dispatch = self.inner.io_dispatch.read();

            let io = match io_dispatch.get(token) {
                Some(io) => io,
                None => return,
            };

            if aba_guard != io.aba_guard {
                return;
            }

            io.readiness.fetch_or(ready.as_usize(), Relaxed);

            if ready.is_writable() || platform::is_hup(ready) {
                wr = io.writer.take_waker();
            }

            if !(ready & (!mio::Ready::writable())).is_empty() {
                rd = io.reader.take_waker();
            }
        }

        if let Some(w) = rd {
            w.wake();
        }

        if let Some(w) = wr {
            w.wake();
        }
    }
}

#[cfg(all(unix, not(target_os = "fuchsia")))]
impl AsRawFd for Reactor {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.io.as_raw_fd()
    }
}

impl Park for Reactor {
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
}

impl fmt::Debug for Reactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Reactor")
    }
}

// ===== impl Handle =====

impl Handle {
    #[doc(hidden)]
    #[deprecated(note = "semantics were sometimes surprising, use Handle::default()")]
    pub fn current() -> Handle {
        // TODO: Should this panic on error?
        HandlePriv::try_current()
            .map(|handle| Handle {
                inner: Some(handle),
            })
            .unwrap_or(Handle {
                inner: Some(HandlePriv { inner: Weak::new() }),
            })
    }

    pub(crate) fn as_priv(&self) -> Option<&HandlePriv> {
        self.inner.as_ref()
    }
}

impl Unpark for Handle {
    fn unpark(&self) {
        if let Some(ref h) = self.inner {
            h.wakeup();
        }
    }
}

impl Default for Handle {
    /// Returns a "default" handle, i.e., a handle that lazily binds to a reactor.
    fn default() -> Handle {
        Handle { inner: None }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}

// ===== impl HandlePriv =====

impl HandlePriv {
    /// Try to get a handle to the current reactor.
    ///
    /// Returns `Err` if no handle is found.
    pub(super) fn try_current() -> io::Result<HandlePriv> {
        CURRENT_REACTOR.with(|current| match *current.borrow() {
            Some(ref handle) => Ok(handle.clone()),
            None => Err(io::Error::new(io::ErrorKind::Other, "no current reactor")),
        })
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

impl fmt::Debug for HandlePriv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HandlePriv")
    }
}

// ===== impl Inner =====

impl Inner {
    /// Register an I/O resource with the reactor.
    ///
    /// The registration token is returned.
    pub(super) fn add_source(&self, source: &dyn Evented) -> io::Result<usize> {
        // Get an ABA guard value
        let aba_guard = self.next_aba_guard.fetch_add(1 << TOKEN_SHIFT, Relaxed);

        let key = {
            // Block to contain the write lock
            let mut io_dispatch = self.io_dispatch.write();

            if io_dispatch.len() == MAX_SOURCES {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "reactor at max \
                     registered I/O resources",
                ));
            }

            io_dispatch.insert(ScheduledIo {
                aba_guard,
                readiness: AtomicUsize::new(0),
                reader: AtomicWaker::new(),
                writer: AtomicWaker::new(),
            })
        };

        let token = aba_guard | key;
        debug!(message = "adding I/O source", token);

        self.io.register(
            source,
            mio::Token(token),
            mio::Ready::all(),
            mio::PollOpt::edge(),
        )?;

        Ok(key)
    }

    /// Deregisters an I/O resource from the reactor.
    pub(super) fn deregister_source(&self, source: &dyn Evented) -> io::Result<()> {
        self.io.deregister(source)
    }

    pub(super) fn drop_source(&self, token: usize) {
        debug!(message = "dropping I/O source", token);
        self.io_dispatch.write().remove(token);
    }

    /// Registers interest in the I/O resource associated with `token`.
    pub(super) fn register(&self, token: usize, dir: Direction, w: Waker) {
        debug!(message = "scheduling", direction = ?dir, token);
        let io_dispatch = self.io_dispatch.read();
        let sched = io_dispatch.get(token).unwrap();

        let (waker, ready) = match dir {
            Direction::Read => (&sched.reader, !mio::Ready::writable()),
            Direction::Write => (&sched.writer, mio::Ready::writable()),
        };

        waker.register(w);

        if sched.readiness.load(SeqCst) & ready.as_usize() != 0 {
            waker.wake();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // When a reactor is dropped it needs to wake up all blocked tasks as
        // they'll never receive a notification, and all connected I/O objects
        // will start returning errors pretty quickly.
        let io = self.io_dispatch.read();
        for (_, io) in io.iter() {
            io.writer.wake();
            io.reader.wake();
        }
    }
}

impl Direction {
    pub(super) fn mask(self) -> mio::Ready {
        match self {
            Direction::Read => {
                // Everything except writable is signaled through read.
                mio::Ready::all() - mio::Ready::writable()
            }
            Direction::Write => mio::Ready::writable() | platform::hup(),
        }
    }
}
