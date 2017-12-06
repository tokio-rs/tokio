//! The core reactor driving all I/O.
//!
//! This module contains the [`Reactor`] reactor type which is the event loop for
//! all I/O happening in `tokio`. This core reactor (or event loop) is used to
//! drive I/O resources.
//!
//! The [`Handle`] and [`Remote`] structs are refences to the event loop,
//! created by the [`handle`][handle_method] and [`remote`][remote_method]
//! respectively, and are used to construct I/O objects. `Remote` is sendable,
//! while `Handle` is not.
//!
//! Lastly [`PollEvented`] can be used to construct I/O objects that interact
//! with the event loop, e.g. [`TcpStream`] in the net module.
//!
//! [`Reactor`]: struct.Reactor.html
//! [`Handle`]: struct.Handle.html
//! [`Remote`]: struct.Remote.html
//! [handle_method]: struct.Reactor.html#method.handle
//! [remote_method]: struct.Reactor.html#method.remote
//! [`PollEvented`]: struct.PollEvented.html
//! [`TcpStream`]: ../net/struct.TcpStream.html

use std::fmt;
use std::io::{self, ErrorKind};
use std::mem;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};
use std::sync::{Arc, Weak, RwLock};
use std::time::{Duration};

use futures::{Future, Async};
use futures::executor::{self, Notify};
use futures::task::{AtomicTask};
use mio;
use mio::event::Evented;
use slab::Slab;

mod io_token;
mod global;

mod poll_evented;
pub use self::poll_evented::PollEvented;

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

    /// Used for determining when the future passed to `run` is ready. Once the
    /// registration is passed to `io` above we never touch it again, just keep
    /// it alive.
    _future_registration: mio::Registration,
    future_readiness: Arc<MySetReadiness>,
}

struct Inner {
    /// The underlying system event queue.
    io: mio::Poll,

    /// Dispatch slabs for I/O and futures events
    io_dispatch: RwLock<Slab<ScheduledIo>>,
}

/// A handle to an event loop.
///
/// A `Handle` is used for associating I/O objects with an event loop
/// explicitly. Typically though you won't end up using a `Handle` that often
/// and will instead use and implicitly configured handle for your thread.
#[derive(Clone)]
pub struct Handle {
    inner: Weak<Inner>,
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
}

impl Reactor {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Reactor> {
        // Create the I/O poller
        let io = try!(mio::Poll::new());

        // Create a registration for unblocking the reactor when the "run"
        // future becomes ready.
        let future_pair = mio::Registration::new2();
        try!(io.register(&future_pair.0,
                         TOKEN_FUTURE,
                         mio::Ready::readable(),
                         mio::PollOpt::level()));

        Ok(Reactor {
            events: mio::Events::with_capacity(1024),
            _future_registration: future_pair.0,
            future_readiness: Arc::new(MySetReadiness(future_pair.1)),
            inner: Arc::new(Inner {
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
        Handle {
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
            io.readiness.fetch_or(ready2usize(ready), Relaxed);
            if ready.is_writable() {
                io.writer.notify();
            }
            if !(ready & (!mio::Ready::writable())).is_empty() {
                io.reader.notify();
            }
        }
    }
}

impl fmt::Debug for Reactor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Reactor")
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // When a reactor is dropped it needs to wake up all blocked tasks as
        // they'll never receive a notification, and all connected I/O objects
        // will start returning errors pretty quickly.
        let io = self.io_dispatch.read().unwrap();
        for (_, io) in io.iter() {
            io.writer.notify();
            io.reader.notify();
        }
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

        if sched.readiness.load(SeqCst) & ready2usize(ready) != 0 {
            task.notify();
        }
    }
}

static HANDLE_FALLBACK: AtomicUsize = ATOMIC_USIZE_INIT;

/// Error returned from `Handle::set_fallback`.
#[derive(Clone, Debug)]
pub struct SetDefaultError(());

impl Handle {
    /// Configures the fallback handle to be returned from `Handle::default`.
    ///
    /// The `Handle::default()` function will by default lazily spin up a global
    /// thread and run a reactor on this global thread. This behavior is not
    /// always desirable in all applications, however, and sometimes a different
    /// fallback reactor is desired.
    ///
    /// This function will attempt to globally alter the return value of
    /// `Handle::default()` to return the `handle` specified rather than a
    /// lazily initialized global thread. If successful then all future calls to
    /// `Handle::default()` which would otherwise fall back to the global thread
    /// will instead return a clone of the handle specified.
    ///
    /// # Errors
    ///
    /// This function may not always succeed in configuring the fallback handle.
    /// If this function was previously called (or perhaps concurrently called
    /// on many threads) only the *first* invocation of this function will
    /// succeed. All other invocations will return an error.
    ///
    /// Additionally if the global reactor thread has already been initialized
    /// then this function will also return an error. (aka if `Handle::default`
    /// has been called previously in this program).
    pub fn set_fallback(handle: Handle) -> Result<(), SetDefaultError> {
        unsafe {
            let val = handle.into_usize();
            match HANDLE_FALLBACK.compare_exchange(0, val, SeqCst, SeqCst) {
                Ok(_) => Ok(()),
                Err(_) => {
                    drop(Handle::from_usize(val));
                    Err(SetDefaultError(()))
                }
            }
        }
    }

    fn into_usize(self) -> usize {
        unsafe {
            mem::transmute::<Weak<Inner>, usize>(self.inner)
        }
    }

    unsafe fn from_usize(val: usize) -> Handle {
        let inner = mem::transmute::<usize, Weak<Inner>>(val);;
        Handle { inner }
    }

    fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

impl Default for Handle {
    fn default() -> Handle {
        let mut fallback = HANDLE_FALLBACK.load(SeqCst);

        // If the fallback hasn't been previously initialized then let's spin
        // up a helper thread and try to initialize with that. If we can't
        // actually create a helper thread then we'll just return a "defunkt"
        // handle which will return errors when I/O objects are attempted to be
        // associated.
        if fallback == 0 {
            let helper = match global::HelperThread::new() {
                Ok(helper) => helper,
                Err(_) => return Handle { inner: Weak::new() },
            };

            // If we successfully set ourselves as the actual fallback then we
            // want to `forget` the helper thread to ensure that it persists
            // globally. If we fail to set ourselves as the fallback that means
            // that someone was racing with this call to `Handle::default`.
            // They ended up winning so we'll destroy our helper thread (which
            // shuts down the thread) and reload the fallback.
            if Handle::set_fallback(helper.handle().clone()).is_ok() {
                let ret = helper.handle().clone();
                helper.forget();
                return ret
            }
            fallback = HANDLE_FALLBACK.load(SeqCst);
        }

        // At this point our fallback handle global was configured so we use
        // its value to reify a handle, clone it, and then forget our reified
        // handle as we don't actually have an owning reference to it.
        assert!(fallback != 0);
        unsafe {
            let handle = Handle::from_usize(fallback);
            let ret = handle.clone();
            drop(handle.into_usize());
            return ret
        }
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
