use crate::loom::sync::atomic::AtomicUsize;
use crate::net::driver::platform;
use crate::runtime::{Park, Unpark};

use std::sync::atomic::Ordering::SeqCst;

mod dispatch;
use dispatch::SingleShard;
pub(crate) use dispatch::MAX_SOURCES;

use mio::event::Evented;
use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
#[cfg(all(unix, not(target_os = "fuchsia")))]
use std::os::unix::io::{AsRawFd, RawFd};
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

pub(super) struct Inner {
    /// The underlying system event queue.
    io: mio::Poll,

    /// Dispatch slabs for I/O and futures events
    // TODO(eliza): once worker threads are available, replace this with a
    // properly sharded slab.
    pub(super) io_dispatch: SingleShard,

    /// The number of sources in `io_dispatch`.
    n_sources: AtomicUsize,

    /// Used to wake up the reactor from a call to `turn`
    wakeup: mio::SetReadiness,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub(super) enum Direction {
    Read,
    Write,
}

thread_local! {
    /// Tracks the reactor for the current execution context.
    static CURRENT_REACTOR: RefCell<Option<Handle>> = RefCell::new(None)
}

const TOKEN_WAKEUP: mio::Token = mio::Token(MAX_SOURCES);

fn _assert_kinds() {
    fn _assert<T: Send + Sync>() {}

    _assert::<Handle>();
}

// ===== impl Reactor =====

#[derive(Debug)]
/// Guard that resets current reactor on drop.
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

/// Sets handle for a default reactor, returning guard that unsets it on drop.
pub fn set_default(handle: &Handle) -> DefaultGuard<'_> {
    CURRENT_REACTOR.with(|current| {
        let mut current = current.borrow_mut();

        assert!(
            current.is_none(),
            "default Tokio reactor already set \
             for execution context"
        );

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
                io_dispatch: SingleShard::new(),
                n_sources: AtomicUsize::new(0),
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
        self.inner.n_sources.load(SeqCst) == 0
    }

    fn poll(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        // Block waiting for an event to happen, peeling out how many events
        // happened.
        match self.inner.io.poll(&mut self.events, max_wait) {
            Ok(_) => {}
            Err(e) => return Err(e),
        }

        // Process all the events that came in, dispatching appropriately

        for event in self.events.iter() {
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

        Ok(())
    }

    fn dispatch(&self, token: mio::Token, ready: mio::Ready) {
        let mut rd = None;
        let mut wr = None;

        let io = match self.inner.io_dispatch.get(token.0) {
            Some(io) => io,
            None => return,
        };

        if io
            .set_readiness(token.0, |curr| curr | ready.as_usize())
            .is_err()
        {
            // token no longer valid!
            return;
        }

        if ready.is_writable() || platform::is_hup(ready) {
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
    /// Returns a handle to the current reactor
    ///
    /// # Panics
    ///
    /// This function panics if there is no current reactor set.
    pub(super) fn current() -> Self {
        CURRENT_REACTOR.with(|current| match *current.borrow() {
            Some(ref handle) => handle.clone(),
            None => panic!("no current reactor"),
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
    /// Register an I/O resource with the reactor.
    ///
    /// The registration token is returned.
    pub(super) fn add_source(&self, source: &dyn Evented) -> io::Result<usize> {
        let token = self.io_dispatch.alloc().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "reactor at max registered I/O resources",
            )
        })?;
        self.n_sources.fetch_add(1, SeqCst);
        self.io.register(
            source,
            mio::Token(token),
            mio::Ready::all(),
            mio::PollOpt::edge(),
        )?;

        Ok(token)
    }

    /// Deregisters an I/O resource from the reactor.
    pub(super) fn deregister_source(&self, source: &dyn Evented) -> io::Result<()> {
        self.io.deregister(source)
    }

    pub(super) fn drop_source(&self, token: usize) {
        self.io_dispatch.remove(token);
        self.n_sources.fetch_sub(1, SeqCst);
    }

    /// Registers interest in the I/O resource associated with `token`.
    pub(super) fn register(&self, token: usize, dir: Direction, w: Waker) {
        let sched = self
            .io_dispatch
            .get(token)
            .unwrap_or_else(|| panic!("IO resource for token {} does not exist!", token));
        let readiness = sched
            .get_readiness(token)
            .unwrap_or_else(|| panic!("token {} no longer valid!", token));

        let (waker, ready) = match dir {
            Direction::Read => (&sched.reader, !mio::Ready::writable()),
            Direction::Write => (&sched.writer, mio::Ready::writable()),
        };

        waker.register(w);
        if readiness & ready.as_usize() != 0 {
            waker.wake();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // When a reactor is dropped it needs to wake up all blocked tasks as
        // they'll never receive a notification, and all connected I/O objects
        // will start returning errors pretty quickly.
        for io in self.io_dispatch.unique_iter() {
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
            println!("\n--- iteration ---\n");
            let reactor = Reactor::new().unwrap();
            let inner = reactor.inner;
            let inner2 = inner.clone();

            let token_1 = inner.add_source(&NotEvented).unwrap();
            println!("token 1: {:#x}", token_1);
            let thread = thread::spawn(move || {
                inner2.drop_source(token_1);
                println!("dropped: {:#x}", token_1);
            });

            let token_2 = inner.add_source(&NotEvented).unwrap();
            println!("token 2: {:#x}", token_2);
            thread.join().unwrap();

            assert!(token_1 != token_2);
        })
    }

    #[test]
    fn tokens_unique_when_dropped_on_full_page() {
        loom::model(|| {
            println!("\n--- iteration ---\n");
            let reactor = Reactor::new().unwrap();
            let inner = reactor.inner;
            let inner2 = inner.clone();
            // add sources to fill up the first page so that the dropped index
            // may be reused.
            for _ in 0..31 {
                inner.add_source(&NotEvented).unwrap();
            }

            let token_1 = inner.add_source(&NotEvented).unwrap();
            println!("token 1: {:#x}", token_1);
            let thread = thread::spawn(move || {
                inner2.drop_source(token_1);
                println!("dropped: {:#x}", token_1);
            });

            let token_2 = inner.add_source(&NotEvented).unwrap();
            println!("token 2: {:#x}", token_2);
            thread.join().unwrap();

            assert!(token_1 != token_2);
        })
    }

    #[test]
    fn tokens_unique_concurrent_add() {
        loom::model(|| {
            println!("\n--- iteration ---\n");
            let reactor = Reactor::new().unwrap();
            let inner = reactor.inner;
            let inner2 = inner.clone();

            let thread = thread::spawn(move || {
                let token_2 = inner2.add_source(&NotEvented).unwrap();
                println!("token 2: {:#x}", token_2);
                token_2
            });

            let token_1 = inner.add_source(&NotEvented).unwrap();
            println!("token 1: {:#x}", token_1);
            let token_2 = thread.join().unwrap();

            assert!(token_1 != token_2);
        })
    }
}
