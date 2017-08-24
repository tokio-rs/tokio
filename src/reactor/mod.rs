//! The core reactor driving all I/O
//!
//! This module contains the `Reactor` type which is the reactor for all I/O
//! happening in `tokio-core`. This reactor (or event loop) is used to run
//! futures, schedule tasks, issue I/O requests, etc.

use std::fmt;
use std::io::{self, ErrorKind};
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, Duration};

use futures::executor;
use futures::task::AtomicTask;
use log::LogLevel;
use mio::event::Evented;
use mio;

use atomic_slab::AtomicSlab;

mod poll_evented;
pub use self::poll_evented::PollEvented;

// An event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
// TODO: expand this
pub struct Reactor {
    inner: Arc<Inner>,
    handle: Handle,
    events: mio::Events,
    _wakeup_registration: mio::Registration,
}

struct Inner {
    // Actual event loop itself, aka an "epoll set"
    io: mio::Poll,

    // All known I/O objects and the tasks that are blocked on them. This slab
    // gives each scheduled I/O an index which is used to refer to it elsewhere.
    io_dispatch: Arc<AtomicSlab<Arc<ScheduledIo>>>,

    // The default readiness for all new I/O objects, this is used to turn I/O
    // objects defunkt during the destructor of `Reactor`.
    default_readiness: AtomicUsize,

    // Used to wake up the reactor from a call to `turn`
    wakeup: mio::SetReadiness,
}

fn _assert() {
    fn _assert<T: Send + Sync>() {}
    _assert::<Reactor>();
    _assert::<Handle>();
}

/// Handle to an event loop, used to construct I/O objects, send messages, and
/// otherwise interact indirectly with the event loop itself.
///
/// Handles can be cloned, and when cloned they will still refer to the
/// same underlying event loop.
#[derive(Clone)]
#[allow(deprecated)]
pub struct Handle {
    repr: HandleRepr,
}

#[derive(Clone)]
enum HandleRepr {
    Ptr { inner: Weak<Inner> },
    Global,
}

struct ScheduledIo {
    readiness: AtomicUsize,
    reader: AtomicTask,
    writer: AtomicTask,
    idx: AtomicUsize,
    slab: Weak<AtomicSlab<Arc<ScheduledIo>>>,
}

const TOKEN_WAKEUP: mio::Token = mio::Token(0);
const TOKEN_START: usize = 1;

impl Reactor {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Reactor> {
        let io = mio::Poll::new()?;
        let future_pair = mio::Registration::new2();
        io.register(&future_pair.0,
                    TOKEN_WAKEUP,
                    mio::Ready::readable(),
                    mio::PollOpt::level())?;

        let inner = Arc::new(Inner {
            io: io,
            io_dispatch: Arc::new(AtomicSlab::new()),
            default_readiness: AtomicUsize::new(0),
            wakeup: future_pair.1,
        });

        Ok(Reactor {
            handle: Handle {
                repr: HandleRepr::Ptr {
                    inner: Arc::downgrade(&inner),
                },
            },
            events: mio::Events::with_capacity(1024),
            inner: inner,
            _wakeup_registration: future_pair.0,
        })
    }

    /// Returns the handle to this event loop which can be used to associate new
    /// I/O objects with this event loop.
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Performs an iteration of this reactor's processing.
    ///
    /// This function will block the current thread until one of the following
    /// occurs:
    ///
    /// * The interval specified by `max_wait` elapses. This condition is never
    ///   met if `max_wait` is specified as `None`.
    /// * A `NotifyHandle` created from this reactor is notified. This id
    ///   done through the `From<&Reactor> for NotifyHandle` implementation
    ///   which is typically used when polling futures.
    /// * An I/O event occurs for any object associated with this reactor.
    ///
    /// When any of the above events occurs this function will internally
    /// dispatch all events for the reactor and then return.
    ///
    /// # Return value
    ///
    /// This function returns a `Turn` value so you may learn why the reactor
    /// returned. Currently this return value only has one method,
    /// `last_notify_id`, which indicates that a `NotifyHandle` created from
    /// this reactor was notified. The `last_notify_id` will return the last id
    /// passed to `notify`.
    ///
    /// # Panics
    ///
    /// This function will panic if an error happens while polling for events
    /// (at the OS layer). This should never happend outside of situations that
    /// indicate a serious application error.
    pub fn turn(&mut self, max_wait: Option<Duration>) -> Turn {
        let _enter = executor::enter();
        self.poll(max_wait);
        Turn { _priv: () }
    }

    fn poll(&mut self, max_wait: Option<Duration>) {
        // Fill in `self.events` with anything that happened, blocking until
        // something occurs.
        match self.inner.io.poll(&mut self.events, max_wait) {
            Ok(_) => (),
            Err(ref e) if e.kind() == ErrorKind::Interrupted => return,
            Err(e) => panic!("error in poll: {}", e),
        }

        let start = if log_enabled!(LogLevel::Debug) {
            Some(Instant::now())
        } else {
            None
        };

        // Process all the events that came in, dispatching appropriately
        let mut events = 0;
        for event in self.events.iter() {
            events += 1;
            let token = event.token();
            trace!("event {:?} {:?}", event.readiness(), event.token());

            if token == TOKEN_WAKEUP {
                self.inner.wakeup.set_readiness(mio::Ready::empty()).unwrap();
            } else {
                self.dispatch(token, event.readiness());
            }
        }
        if let Some(start) = start {
            debug!("loop process - {} events, {:?}", events, start.elapsed());
        }
    }

    fn dispatch(&self, token: mio::Token, ready: mio::Ready) {
        let token = usize::from(token) - TOKEN_START;
        let io = match self.inner.io_dispatch.get(token) {
            Some(io) => io,
            None => return,
        };
        io.readiness.fetch_or(ready2usize(ready), Ordering::Relaxed);
        if ready.is_writable() {
            io.writer.notify();
        }
        if !(ready & (!mio::Ready::writable())).is_empty() {
            io.reader.notify();
        }
    }
}

/// Return value of the `turn` method
pub struct Turn {
    _priv: (),
}

impl Inner {
    fn add_source(&self, source: &Evented) -> io::Result<Arc<ScheduledIo>> {
        debug!("adding a new I/O source");
        let state = Arc::new(ScheduledIo {
            readiness: AtomicUsize::new(0),
            reader: AtomicTask::new(),
            writer: AtomicTask::new(),
            idx: AtomicUsize::new(0),
            slab: Arc::downgrade(&self.io_dispatch),
        });
        let idx = match self.io_dispatch.insert(state.clone()) {
            Some(idx) => idx,
            None => return Err(io::Error::new(io::ErrorKind::Other,
                                              "too many I/O objects registered")),
        };
        state.idx.store(idx, Ordering::SeqCst);

        // Set ourselves to the default readiness. By default this is "nothing
        // ready" but when the reactor is being destroyed this'll get set to
        // MAX to say that it's an invalid object.
        //
        // Note that this happens *after* we successfully store ourselves in
        // the slab to be sure that we see the "now invalid" flag during
        // destruction.
        let readiness = self.default_readiness.load(Ordering::SeqCst);
        state.readiness.store(readiness, Ordering::SeqCst);

        self.io.register(source,
                         mio::Token(TOKEN_START + idx),
                         mio::Ready::readable() |
                             mio::Ready::writable() |
                             platform::all(),
                         mio::PollOpt::edge())?;
        Ok(state)
    }
}

impl fmt::Debug for Reactor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Reactor")
         .finish()
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        // Ensure that all newly registered I/O objects will receive a "defunkt"
        // sentinel for readiness. This means that anything we miss in the below
        // loop will automatically be flagged as "not ready"
        self.inner.default_readiness.store(usize::max_value(), Ordering::SeqCst);

        // Wake up any I/O object that's still blocked, we'll never wake them up
        // again so we need to let them know that they should start immediately
        // returning errors.
        self.inner.io_dispatch.for_each(|io| {
            io.readiness.store(usize::max_value(), Ordering::SeqCst);
            io.reader.notify();
            io.writer.notify();
        });
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
    pub fn wakeup(&self) {
        if let Some(inner) = self.inner() {
            inner.wakeup.set_readiness(mio::Ready::readable()).unwrap();
        }
    }

    fn inner(&self) -> Option<Arc<Inner>> {
        self.repr.inner()
    }
}

impl Default for Handle {
    /// Acquires a handle to the global reactor.
    ///
    /// The global reactor is run in a separate thread in this process and is
    /// lazily initialized. The first invocation of this function will spin up
    /// the reactor.
    ///
    /// The `Handle` returned can be used to register I/O objects with the
    /// reactor and create timeouts.
    fn default() -> Handle {
        Handle { repr: HandleRepr::Global }
    }
}

impl HandleRepr {
    fn inner(&self) -> Option<Arc<Inner>> {
        match *self {
            HandleRepr::Ptr { ref inner, .. } => inner.upgrade(),
            HandleRepr::Global => ::global::reactor().and_then(|x| x.inner()),
        }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Handle")
         .finish()
    }
}

fn read_ready() -> mio::Ready {
    mio::Ready::readable() | platform::hup()
}

const READ: usize = 1 << 0;
const WRITE: usize = 1 << 1;

fn ready2usize(ready: mio::Ready) -> usize {
    let mut bits = 0;
    if ready.is_readable() {
        bits |= READ;
    }
    if ready.is_writable() {
        bits |= WRITE;
    }
    bits | platform::ready2usize(ready)
}

fn usize2ready(bits: usize) -> mio::Ready {
    let mut ready = mio::Ready::empty();
    if bits & READ != 0 {
        ready.insert(mio::Ready::readable());
    }
    if bits & WRITE != 0 {
        ready.insert(mio::Ready::writable());
    }
    ready | platform::usize2ready(bits)
}

#[cfg(all(unix, not(target_os = "fuchsia")))]
mod platform {
    use mio::Ready;
    use mio::unix::UnixReady;

    pub fn aio() -> Ready {
        UnixReady::aio().into()
    }

    pub fn all() -> Ready {
        hup() | aio()
    }

    pub fn hup() -> Ready {
        UnixReady::hup().into()
    }

    const HUP: usize = 1 << 2;
    const ERROR: usize = 1 << 3;
    const AIO: usize = 1 << 4;

    pub fn ready2usize(ready: Ready) -> usize {
        let ready = UnixReady::from(ready);
        let mut bits = 0;
        if ready.is_aio() {
            bits |= AIO;
        }
        if ready.is_error() {
            bits |= ERROR;
        }
        if ready.is_hup() {
            bits |= HUP;
        }
        bits
    }

    pub fn usize2ready(bits: usize) -> Ready {
        let mut ready = UnixReady::from(Ready::empty());
        if bits & AIO != 0 {
            ready.insert(UnixReady::aio());
        }
        if bits & HUP != 0 {
            ready.insert(UnixReady::hup());
        }
        if bits & ERROR != 0 {
            ready.insert(UnixReady::error());
        }
        ready.into()
    }
}

#[cfg(any(windows, target_os = "fuchsia"))]
mod platform {
    use mio::Ready;

    pub fn all() -> Ready {
        // No platform-specific Readinesses for Windows
        Ready::empty()
    }

    pub fn hup() -> Ready {
        Ready::empty()
    }

    pub fn ready2usize(_r: Ready) -> usize {
        0
    }

    pub fn usize2ready(_r: usize) -> Ready {
        Ready::empty()
    }
}
