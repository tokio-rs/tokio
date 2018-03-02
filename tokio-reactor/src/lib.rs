//! Event loop that drives Tokio I/O resources.
//!
//! The reactor is the engine that drives asynchronous I/O resources (like TCP and
//! UDP sockets). It is backed by [`mio`] and acts as a bridge between [`mio`] and
//! [`futures`].
//!
//! The crate provides:
//!
//! * [`Reactor`] is the main type of this crate. It performs the event loop logic.
//!
//! * [`Handle`] provides a reference to a reactor instance.
//!
//! * [`Registration`] and [`PollEvented`] allow third parties to implement I/O
//!   resources that are driven by the reactor.
//!
//! Application authors will not use this crate directly. Instead, they will use the
//! `tokio` crate. Library authors should only depend on `tokio-reactor` if they
//! are building a custom I/O resource.
//!
//! For more details, see [reactor module] documentation in the Tokio crate.
//!
//! [`mio`]: http://github.com/carllerche/mio
//! [`futures`]: http://github.com/rust-lang-nursery/futures-rs
//! [`Reactor`]: struct.Reactor.html
//! [`Handle`]: struct.Handle.html
//! [`Registration`]: struct.Registration.html
//! [`PollEvented`]: struct.PollEvented.html
//! [reactor module]: https://docs.rs/tokio/0.1/tokio/reactor/index.html

#![doc(html_root_url = "https://docs.rs/tokio-reactor/0.1.0")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate tokio_executor;
extern crate tokio_io;

pub(crate) mod background;
mod atomic_task;
mod poll_evented;
mod registration;

// ===== Public re-exports =====

pub use self::background::Background;
pub use self::registration::Registration;
pub use self::poll_evented::PollEvented;

// ===== Private imports =====

use atomic_task::AtomicTask;

use tokio_executor::Enter;
use tokio_executor::park::{Park, Unpark};

use std::{fmt, usize};
use std::io::{self, ErrorKind};
use std::mem;
use std::cell::RefCell;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};
use std::sync::{Arc, Weak, RwLock};
use std::time::{Duration, Instant};

use log::Level;
use mio::event::Evented;
use slab::Slab;
use futures::task::Task;

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
#[derive(Clone)]
pub struct Handle {
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

/// Error returned from `Handle::set_fallback`.
#[derive(Clone, Debug)]
pub struct SetFallbackError(());

#[deprecated(since = "0.1.2", note = "use SetFallbackError instead")]
#[doc(hidden)]
pub type SetDefaultError = SetFallbackError;

struct Inner {
    /// The underlying system event queue.
    io: mio::Poll,

    /// Dispatch slabs for I/O and futures events
    io_dispatch: RwLock<Slab<ScheduledIo>>,

    /// Used to wake up the reactor from a call to `turn`
    wakeup: mio::SetReadiness
}

struct ScheduledIo {
    readiness: AtomicUsize,
    reader: AtomicTask,
    writer: AtomicTask,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub(crate) enum Direction {
    Read,
    Write,
}

/// The global fallback reactor.
static HANDLE_FALLBACK: AtomicUsize = ATOMIC_USIZE_INIT;

/// Tracks the reactor for the current execution context.
thread_local!(static CURRENT_REACTOR: RefCell<Option<Handle>> = RefCell::new(None));

const TOKEN_WAKEUP: mio::Token = mio::Token(0);
const TOKEN_START: usize = 1;

// Kind of arbitrary, but this reserves some token space for later usage.
const MAX_SOURCES: usize = usize::MAX >> 4;

fn _assert_kinds() {
    fn _assert<T: Send + Sync>() {}

    _assert::<Handle>();
}

// ===== impl Reactor =====

/// Set the default reactor for the duration of the closure
///
/// # Panics
///
/// This function panics if there already is a default reactor set.
pub fn with_default<F, R>(handle: &Handle, enter: &mut Enter, f: F) -> R
where F: FnOnce(&mut Enter) -> R
{
    // Ensure that the executor is removed from the thread-local context
    // when leaving the scope. This handles cases that involve panicking.
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT_REACTOR.with(|current| {
                let mut current = current.borrow_mut();
                *current = None;
            });
        }
    }

    // This ensures the value for the current reactor gets reset even if there
    // is a panic.
    let _r = Reset;

    CURRENT_REACTOR.with(|current| {
        {
            let mut current = current.borrow_mut();
            assert!(current.is_none(), "default Tokio reactor already set \
                    for execution context");
            *current = Some(handle.clone());
        }

        f(enter)
    })
}

impl Reactor {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Reactor> {
        let io = mio::Poll::new()?;
        let wakeup_pair = mio::Registration::new2();

        io.register(&wakeup_pair.0,
                    TOKEN_WAKEUP,
                    mio::Ready::readable(),
                    mio::PollOpt::level())?;

        Ok(Reactor {
            events: mio::Events::with_capacity(1024),
            _wakeup_registration: wakeup_pair.0,
            inner: Arc::new(Inner {
                io: io,
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
            inner: Arc::downgrade(&self.inner),
        }
    }

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
    pub fn set_fallback(&self) -> Result<(), SetFallbackError> {
        set_fallback(self.handle())
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
        self.inner.io_dispatch
            .read().unwrap()
            .is_empty()
    }

    /// Run this reactor on a background thread.
    ///
    /// This function takes ownership, spawns a new thread, and moves the
    /// reactor to this new thread. It then runs the reactor, driving all
    /// associated I/O resources, until the `Background` handle is dropped or
    /// explicitly shutdown.
    pub fn background(self) -> io::Result<Background> {
        Background::new(self)
    }

    fn poll(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        // Block waiting for an event to happen, peeling out how many events
        // happened.
        match self.inner.io.poll(&mut self.events, max_wait) {
            Ok(_) => {}
            Err(ref e) if e.kind() == ErrorKind::Interrupted => return Ok(()),
            Err(e) => return Err(e),
        }

        let start = if log_enabled!(Level::Debug) {
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
            let dur = start.elapsed();
            debug!("loop process - {} events, {}.{:03}s",
                   events,
                   dur.as_secs(),
                   dur.subsec_nanos() / 1_000_000);
        }

        Ok(())
    }

    fn dispatch(&self, token: mio::Token, ready: mio::Ready) {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Reactor")
    }
}

// ===== impl Handle =====

impl Handle {
    /// Returns a handle to the current reactor.
    pub fn current() -> Handle {
        Handle::try_current()
            .unwrap_or(Handle { inner: Weak::new() })
    }

    /// Try to get a handle to the current reactor.
    ///
    /// Returns `Err` if no handle is found.
    pub(crate) fn try_current() -> io::Result<Handle> {
        CURRENT_REACTOR.with(|current| {
            match *current.borrow() {
                Some(ref handle) => Ok(handle.clone()),
                None => Handle::fallback(),
            }
        })
    }

    /// Returns a handle to the fallback reactor.
    fn fallback() -> io::Result<Handle> {
        let mut fallback = HANDLE_FALLBACK.load(SeqCst);

        // If the fallback hasn't been previously initialized then let's spin
        // up a helper thread and try to initialize with that. If we can't
        // actually create a helper thread then we'll just return a "defunct"
        // handle which will return errors when I/O objects are attempted to be
        // associated.
        if fallback == 0 {
            let reactor = match Reactor::new() {
                Ok(reactor) => reactor,
                Err(_) => return Err(io::Error::new(io::ErrorKind::Other,
                                                    "failed to create reactor")),
            };

            // If we successfully set ourselves as the actual fallback then we
            // want to `forget` the helper thread to ensure that it persists
            // globally. If we fail to set ourselves as the fallback that means
            // that someone was racing with this call to `Handle::default`.
            // They ended up winning so we'll destroy our helper thread (which
            // shuts down the thread) and reload the fallback.
            if set_fallback(reactor.handle().clone()).is_ok() {
                let ret = reactor.handle().clone();

                match reactor.background() {
                    Ok(bg) => bg.forget(),
                    // The global handle is fubar, but y'all probably got bigger
                    // problems if a thread can't spawn.
                    Err(_) => {}
                }

                return Ok(ret);
            }

            fallback = HANDLE_FALLBACK.load(SeqCst);
        }

        // At this point our fallback handle global was configured so we use
        // its value to reify a handle, clone it, and then forget our reified
        // handle as we don't actually have an owning reference to it.
        assert!(fallback != 0);

        let ret = unsafe {
            let handle = Handle::from_usize(fallback);
            let ret = handle.clone();
            drop(handle.into_usize());
            ret
        };

        Ok(ret)
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

impl Unpark for Handle {
    fn unpark(&self) {
        self.wakeup();
    }
}

impl Default for Handle {
    fn default() -> Handle {
        Handle::current()
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Handle")
    }
}

fn set_fallback(handle: Handle) -> Result<(), SetFallbackError> {
    unsafe {
        let val = handle.into_usize();
        match HANDLE_FALLBACK.compare_exchange(0, val, SeqCst, SeqCst) {
            Ok(_) => Ok(()),
            Err(_) => {
                drop(Handle::from_usize(val));
                Err(SetFallbackError(()))
            }
        }
    }
}

// ===== impl Inner =====

impl Inner {
    /// Register an I/O resource with the reactor.
    ///
    /// The registration token is returned.
    fn add_source(&self, source: &Evented)
        -> io::Result<usize>
    {
        let mut io_dispatch = self.io_dispatch.write().unwrap();

        if io_dispatch.len() == MAX_SOURCES {
            return Err(io::Error::new(io::ErrorKind::Other, "reactor at max \
                                      registered I/O resources"));
        }

        // Acquire a write lock
        let key = io_dispatch.insert(ScheduledIo {
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

    fn drop_source(&self, token: usize) {
        debug!("dropping I/O source: {}", token);
        self.io_dispatch.write().unwrap().remove(token);
    }

    /// Registers interest in the I/O resource associated with `token`.
    fn register(&self, token: usize, dir: Direction, t: Task) {
        debug!("scheduling direction for: {}", token);
        let io_dispatch = self.io_dispatch.read().unwrap();
        let sched = io_dispatch.get(token).unwrap();

        let (task, ready) = match dir {
            Direction::Read => (&sched.reader, !mio::Ready::writable()),
            Direction::Write => (&sched.writer, mio::Ready::writable()),
        };

        task.register_task(t);

        if sched.readiness.load(SeqCst) & ready2usize(ready) != 0 {
            task.notify();
        }
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

impl Direction {
    fn ready(&self) -> mio::Ready {
        match *self {
            Direction::Read => read_ready(),
            Direction::Write => write_ready(),
        }
    }

    fn mask(&self) -> usize {
        ready2usize(self.ready())
    }
}

// ===== misc =====

const READ: usize = 1 << 0;
const WRITE: usize = 1 << 1;

fn read_ready() -> mio::Ready {
    mio::Ready::readable() | platform::hup()
}

fn write_ready() -> mio::Ready {
    mio::Ready::writable()
}

// === legacy

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

    #[cfg(target_os = "dragonfly")]
    pub fn all() -> Ready {
        hup() | UnixReady::aio()
    }

    #[cfg(target_os = "freebsd")]
    pub fn all() -> Ready {
        hup() | UnixReady::aio() | UnixReady::lio()
    }

    #[cfg(not(any(target_os = "dragonfly", target_os = "freebsd")))]
    pub fn all() -> Ready {
        hup()
    }

    pub fn hup() -> Ready {
        UnixReady::hup().into()
    }

    const HUP: usize = 1 << 2;
    const ERROR: usize = 1 << 3;
    const AIO: usize = 1 << 4;
    const LIO: usize = 1 << 5;

    #[cfg(any(target_os = "dragonfly", target_os = "freebsd"))]
    fn is_aio(ready: &Ready) -> bool {
        UnixReady::from(*ready).is_aio()
    }

    #[cfg(not(any(target_os = "dragonfly", target_os = "freebsd")))]
    fn is_aio(_ready: &Ready) -> bool {
        false
    }

    #[cfg(target_os = "freebsd")]
    fn is_lio(ready: &Ready) -> bool {
        UnixReady::from(*ready).is_lio()
    }

    #[cfg(not(target_os = "freebsd"))]
    fn is_lio(_ready: &Ready) -> bool {
        false
    }

    pub fn ready2usize(ready: Ready) -> usize {
        let ready = UnixReady::from(ready);
        let mut bits = 0;
        if is_aio(&ready) {
            bits |= AIO;
        }
        if is_lio(&ready) {
            bits |= LIO;
        }
        if ready.is_error() {
            bits |= ERROR;
        }
        if ready.is_hup() {
            bits |= HUP;
        }
        bits
    }

    #[cfg(any(target_os = "dragonfly", target_os = "freebsd", target_os = "ios",
              target_os = "macos"))]
    fn usize2ready_aio(ready: &mut UnixReady) {
        ready.insert(UnixReady::aio());
    }

    #[cfg(not(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos")))]
    fn usize2ready_aio(_ready: &mut UnixReady) {
        // aio not available here → empty
    }

    #[cfg(target_os = "freebsd")]
    fn usize2ready_lio(ready: &mut UnixReady) {
        ready.insert(UnixReady::lio());
    }

    #[cfg(not(target_os = "freebsd"))]
    fn usize2ready_lio(_ready: &mut UnixReady) {
        // lio not available here → empty
    }

    pub fn usize2ready(bits: usize) -> Ready {
        let mut ready = UnixReady::from(Ready::empty());
        if bits & AIO != 0 {
            usize2ready_aio(&mut ready);
        }
        if bits & LIO != 0 {
            usize2ready_lio(&mut ready);
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
