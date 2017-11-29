//! The core reactor driving all I/O
//!
//! This module contains the `Core` type which is the reactor for all I/O
//! happening in `tokio-core`. This reactor (or event loop) is used to drive I/O
//! resources.

use std::fmt;
use std::io::{self, ErrorKind};
use std::sync::{Arc, Weak, RwLock};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::time::{Duration};

use futures::{Future, Async};
use futures::executor::{self, Notify};
use futures::task::{AtomicTask};
use mio;
use mio::event::Evented;
use slab::Slab;

mod io_token;

mod poll_evented;
pub use self::poll_evented::PollEvented;

/// Global counter used to assign unique IDs to reactor instances.
static NEXT_LOOP_ID: AtomicUsize = ATOMIC_USIZE_INIT;

/// An event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
pub struct Core {
    /// Reuse the `mio::Events` value across calls to poll.
    events: mio::Events,

    /// State shared between the reactor and the handles.
    inner: Arc<Inner>,

    /// Used for determining when the future passed to `run` is ready. Once the
    /// registration is passed to `io` above we never touch it again, just keep
    /// it alive.
    _future_registration: mio::Registration,
    future_readiness: Arc<MySetReadiness>,
}

struct Inner {
    /// Unique identifier referencing this reactor.
    id: usize,

    /// The underlying system event queue.
    io: mio::Poll,

    /// Dispatch slabs for I/O and futures events
    io_dispatch: RwLock<Slab<ScheduledIo>>,
}

/// Handle to an event loop, used to construct I/O objects, send messages, and
/// otherwise interact indirectly with the event loop itself.
///
/// Handles can be cloned, and when cloned they will still refer to the
/// same underlying event loop.
#[derive(Clone)]
pub struct Remote {
    id: usize,
    inner: Weak<Inner>,
}

/// A non-sendable handle to an event loop, useful for manufacturing instances
/// of `LoopData`.
#[derive(Clone)]
pub struct Handle {
    remote: Remote,
}

struct ScheduledIo {
    readiness: AtomicUsize,
    reader: AtomicTask,
    writer: AtomicTask,
}

enum Direction {
    Read,
    Write,
}

const TOKEN_FUTURE: mio::Token = mio::Token(1);
const TOKEN_START: usize = 2;

fn _assert_kinds() {
    fn _assert<T: Send + Sync>() {}

    _assert::<Handle>();
    _assert::<Remote>();
}

impl Core {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Core> {
        // Create the I/O poller
        let io = try!(mio::Poll::new());

        // Create a registration for unblocking the reactor when the "run"
        // future becomes ready.
        let future_pair = mio::Registration::new2();
        try!(io.register(&future_pair.0,
                         TOKEN_FUTURE,
                         mio::Ready::readable(),
                         mio::PollOpt::level()));

        Ok(Core {
            events: mio::Events::with_capacity(1024),
            _future_registration: future_pair.0,
            future_readiness: Arc::new(MySetReadiness(future_pair.1)),
            inner: Arc::new(Inner {
                id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
                io: io,
                io_dispatch: RwLock::new(Slab::with_capacity(1)),
            }),
        })
    }

    /// Returns a handle to this event loop which cannot be sent across threads
    /// but can be used as a proxy to the event loop itself.
    ///
    /// Handles are cloneable and clones always refer to the same event loop.
    /// This handle is typically passed into functions that create I/O objects
    /// to bind them to this event loop.
    pub fn handle(&self) -> Handle {
        let remote = self.remote();
        Handle { remote }
    }

    /// Generates a remote handle to this event loop which can be used to spawn
    /// tasks from other threads into this event loop.
    pub fn remote(&self) -> Remote {
        Remote {
            id: self.inner.id,
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Runs a future until completion, driving the event loop while we're
    /// otherwise waiting for the future to complete.
    ///
    /// This function will begin executing the event loop and will finish once
    /// the provided future is resolved. Note that the future argument here
    /// crucially does not require the `'static` nor `Send` bounds. As a result
    /// the future will be "pinned" to not only this thread but also this stack
    /// frame.
    ///
    /// This function will return the value that the future resolves to once
    /// the future has finished. If the future never resolves then this function
    /// will never return.
    ///
    /// # Panics
    ///
    /// This method will **not** catch panics from polling the future `f`. If
    /// the future panics then it's the responsibility of the caller to catch
    /// that panic and handle it as appropriate.
    pub fn run<F>(&mut self, f: F) -> Result<F::Item, F::Error>
        where F: Future,
    {
        let mut task = executor::spawn(f);
        let mut future_fired = true;

        loop {
            if future_fired {
                let res = task.poll_future_notify(&self.future_readiness, 0)?;
                if let Async::Ready(e) = res {
                    return Ok(e)
                }
            }
            future_fired = self.poll(None);
        }
    }

    /// Performs one iteration of the event loop, blocking on waiting for events
    /// for at most `max_wait` (forever if `None`).
    ///
    /// It only makes sense to call this method if you've previously spawned
    /// a future onto this event loop.
    ///
    /// `loop { lp.turn(None) }` is equivalent to calling `run` with an
    /// empty future (one that never finishes).
    pub fn turn(&mut self, max_wait: Option<Duration>) {
        self.poll(max_wait);
    }

    fn poll(&mut self, max_wait: Option<Duration>) -> bool {
        // Block waiting for an event to happen, peeling out how many events
        // happened.
        match self.inner.io.poll(&mut self.events, max_wait) {
            Ok(_) => {}
            Err(ref e) if e.kind() == ErrorKind::Interrupted => return false,
            // TODO: This should return an io::Result instead of panic.
            Err(e) => panic!("error in poll: {}", e),
        }

        // Process all the events that came in, dispatching appropriately
        let mut fired = false;
        for i in 0..self.events.len() {
            let event = self.events.get(i).unwrap();
            let token = event.token();
            trace!("event {:?} {:?}", event.readiness(), event.token());

            if token == TOKEN_FUTURE {
                self.future_readiness.0.set_readiness(mio::Ready::empty()).unwrap();
                fired = true;
            } else {
                self.dispatch(token, event.readiness());
            }
        }

        return fired
    }

    fn dispatch(&mut self, token: mio::Token, ready: mio::Ready) {
        let token = usize::from(token) - TOKEN_START;
        let io_dispatch = self.inner.io_dispatch.read().unwrap();

        if let Some(io) = io_dispatch.get(token) {
            io.readiness.fetch_or(ready2usize(ready), Ordering::Relaxed);
            if ready.is_writable() {
                io.writer.notify();
            }
            if !(ready & (!mio::Ready::writable())).is_empty() {
                io.reader.notify();
            }
        }
    }
}

impl fmt::Debug for Core {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Core")
    }
}

impl Inner {
    /// Register an I/O resource with the reactor.
    ///
    /// The registration token is returned.
    fn add_source(&self, source: &Evented)
        -> io::Result<usize>
    {
        // Acquire a write lock
        let key = self.io_dispatch.write().unwrap()
            .insert(ScheduledIo {
                readiness: AtomicUsize::new(0),
                reader: AtomicTask::new(),
                writer: AtomicTask::new(),
            });

        try!(self.io.register(source,
                              mio::Token(TOKEN_START + key),
                              mio::Ready::readable() |
                                mio::Ready::writable() |
                                platform::all(),
                              mio::PollOpt::edge()));

        Ok(key)
    }

    fn deregister_source(&self, source: &Evented) -> io::Result<()> {
        self.io.deregister(source)
    }

    fn drop_source(&self, token: usize) {
        debug!("dropping I/O source: {}", token);
        self.io_dispatch.write().unwrap().remove(token);
    }

    /// Registers interest in the I/O resource associated with `token`.
    fn schedule(&self, token: usize, dir: Direction) {
        debug!("scheduling direction for: {}", token);
        let io_dispatch = self.io_dispatch.read().unwrap();
        let sched = io_dispatch.get(token).unwrap();

        let (task, ready) = match dir {
            Direction::Read => (&sched.reader, !mio::Ready::writable()),
            Direction::Write => (&sched.writer, mio::Ready::writable()),
        };

        task.register();

        if sched.readiness.load(Ordering::SeqCst) & ready2usize(ready) != 0 {
            task.notify();
        }
    }
}

impl Remote {
    /// Attempts to "promote" this remote to a handle, if possible.
    ///
    /// This function is intended for structures which typically work through a
    /// `Remote` but want to optimize runtime when the remote doesn't actually
    /// leave the thread of the original reactor. This will attempt to return a
    /// handle if the `Remote` is on the same thread as the event loop and the
    /// event loop is running.
    ///
    /// If this `Remote` has moved to a different thread or if the event loop is
    /// running, then `None` may be returned. If you need to guarantee access to
    /// a `Handle`, then you can call this function and fall back to using
    /// `spawn` above if it returns `None`.
    pub fn handle(&self) -> Option<Handle> {
        let remote = self.clone();
        Some(Handle { remote } )
    }

    /// Spawns a new future into the event loop this remote is associated with.
    ///
    /// This function takes a closure which is executed within the context of
    /// the I/O loop itself. The future returned by the closure will be
    /// scheduled on the event loop and run to completion.
    ///
    /// Note that while the closure, `F`, requires the `Send` bound as it might
    /// cross threads, the future `R` does not.
    ///
    /// # Panics
    ///
    /// This method will **not** catch panics from polling the future `f`. If
    /// the future panics then it's the responsibility of the caller to catch
    /// that panic and handle it as appropriate.
    pub(crate) fn run<F>(&self, f: F)
        where F: FnOnce(&Handle) + Send + 'static,
    {
        let handle = self.handle().unwrap();
        f(&handle);
    }
}

impl fmt::Debug for Remote {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"Remote")
    }
}

impl Handle {
    /// Returns a reference to the underlying remote handle to the event loop.
    pub fn remote(&self) -> &Remote {
        &self.remote
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Handle")
    }
}

struct MySetReadiness(mio::SetReadiness);

impl Notify for MySetReadiness {
    fn notify(&self, _id: usize) {
        self.0.set_readiness(mio::Ready::readable())
              .expect("failed to set readiness");
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
