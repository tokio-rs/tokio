pub(crate) mod platform;

mod scheduled_io;
pub(crate) use scheduled_io::ScheduledIo; // pub(crate) for tests

use crate::loom::sync::atomic::AtomicUsize;
use crate::park::{Park, Unpark};
use crate::runtime::context;
use crate::util::slab::{Address, Slab};

use mio::event::Evented;
use std::fmt;
use std::io;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Weak};
use std::task::Waker;
use std::time::Duration;

/// I/O driver, backed by Mio
pub(crate) struct Driver {
    /// Reuse the `mio::Events` value across calls to poll.
    events: mio::Events,

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

    /// Dispatch slabs for I/O and futures events
    pub(super) io_dispatch: Slab<ScheduledIo>,

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

const TOKEN_WAKEUP: mio::Token = mio::Token(Address::NULL);

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

        io.register(
            &wakeup_pair.0,
            TOKEN_WAKEUP,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )?;

        Ok(Driver {
            events: mio::Events::with_capacity(1024),
            _wakeup_registration: wakeup_pair.0,
            inner: Arc::new(Inner {
                io,
                io_dispatch: Slab::new(),
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
    pub(crate) fn handle(&self) -> Handle {
        Handle {
            inner: Arc::downgrade(&self.inner),
        }
    }

    fn turn(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
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

        let address = Address::from_usize(token.0);

        let io = match self.inner.io_dispatch.get(address) {
            Some(io) => io,
            None => return,
        };

        if io
            .set_readiness(address, |curr| curr | ready.as_usize())
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
    /// Registers an I/O resource with the reactor.
    ///
    /// The registration token is returned.
    pub(super) fn add_source(&self, source: &dyn Evented) -> io::Result<Address> {
        let address = self.io_dispatch.alloc().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "reactor at max registered I/O resources",
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

    /// Deregisters an I/O resource from the reactor.
    pub(super) fn deregister_source(&self, source: &dyn Evented) -> io::Result<()> {
        self.io.deregister(source)
    }

    pub(super) fn drop_source(&self, address: Address) {
        self.io_dispatch.remove(address);
        self.n_sources.fetch_sub(1, SeqCst);
    }

    /// Registers interest in the I/O resource associated with `token`.
    pub(super) fn register(&self, token: Address, dir: Direction, w: Waker) {
        let sched = self
            .io_dispatch
            .get(token)
            .unwrap_or_else(|| panic!("IO resource for token {:?} does not exist!", token));

        let waker = match dir {
            Direction::Read => &sched.reader,
            Direction::Write => &sched.writer,
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
