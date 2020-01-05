//! I/O Driver backed by Mio
pub(crate) mod platform;
mod registration;
mod scheduled_io;
use crate::future::poll_fn;
use crate::io::PollEvented;
use crate::loom::sync::atomic::AtomicUsize;
use crate::net::tcp::{listener::ListenerInner, stream::StreamInner};
use crate::park::{Park, Unpark};
use crate::util::slab::{Address, Slab};
use mio::event::Evented;
pub(crate) use registration::Direction;
pub use registration::Registration;
pub(crate) use scheduled_io::ScheduledIo; // pub(crate) for tests
use std::fmt;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Weak};
use std::task::Waker;
use std::time::Duration;
use std::{io, net, vec};

#[derive(Debug, Clone)]
pub(crate) enum Handle {
    Mio(MioHandle),
    Simulation(crate::simulation::SimulationHandle),
}

impl Handle {
    pub(crate) fn resolve_str_addr(&self, s: &str) -> io::Result<vec::IntoIter<net::SocketAddr>> {
        let s = s.to_owned();
        match self {
            Handle::Mio(_) => std::net::ToSocketAddrs::to_socket_addrs(&s),
            Handle::Simulation(_) => {
                fn err(msg: &str) -> io::Error {
                    io::Error::new(io::ErrorKind::InvalidInput, msg)
                }
                let mut parts_iter = s.rsplitn(2, ':');
                let port_str = parts_iter.next().ok_or(err("invalid socket address"))?;
                let host = parts_iter.next().ok_or(err("invalid socket address"))?;
                let port: u16 = port_str.parse().map_err(|_| err("invalid port value"))?;
                self.resolve_tuple_addr(&(host, port))
            }
        }
    }

    pub(crate) fn resolve_tuple_addr(
        &self,
        addr: &(&str, u16),
    ) -> io::Result<vec::IntoIter<net::SocketAddr>> {
        match self {
            Handle::Mio(_) => std::net::ToSocketAddrs::to_socket_addrs(addr),
            Handle::Simulation(sim) => sim.resolve_tuple(addr),
        }
    }

    /// Kept around for types which do not have a simulation equivalent yet.
    pub(crate) fn register_io<T>(&self, io: &T) -> io::Result<registration::Registration>
    where
        T: Evented,
    {
        match self {
            Handle::Mio(mio) => mio.register_io(io),
            Handle::Simulation(_) => panic!("cannot register std or mio io with simulation handle"),
        }
    }

    /// Establish a connection to the specified `addr`.
    pub(crate) async fn tcp_stream_connect_addr(
        &self,
        addr: net::SocketAddr,
    ) -> io::Result<StreamInner> {
        match self {
            Handle::Mio(mio) => {
                let sys = mio::net::TcpStream::connect(&addr)?;
                let registration = mio.register_io(&sys)?;
                let io = PollEvented::new(sys, registration)?;

                poll_fn(|cx| io.poll_write_ready(cx)).await?;

                if let Some(e) = io.get_ref().take_error()? {
                    return Err(e);
                }
                Ok(StreamInner::Mio(io))
            }
            Handle::Simulation(sim) => sim.connect(addr).await.map(|s| StreamInner::Sim(s)),
        }
    }

    pub(crate) fn tcp_listener_bind_addr(
        &self,
        addr: net::SocketAddr,
    ) -> io::Result<ListenerInner> {
        match self {
            Handle::Mio(mio) => {
                let listener = mio::net::TcpListener::bind(&addr)?;
                let registration = mio.register_io(&listener)?;
                let io = PollEvented::new(listener, registration)?;
                Ok(ListenerInner::Mio(io))
            }
            Handle::Simulation(sim) => sim.bind(addr.port()).map(|s| ListenerInner::Sim(s)),
        }
    }
}

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

    pub(crate) fn handle(&self) -> MioHandle {
        let inner = Arc::downgrade(&self.inner);
        MioHandle::new(inner)
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

#[derive(Clone)]
pub(crate) struct MioHandle {
    inner: Weak<Inner>,
}

impl MioHandle {
    fn new(inner: Weak<Inner>) -> Self {
        MioHandle { inner }
    }

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
        let handle = MioHandle::new(Arc::downgrade(&inner));
        Ok(Registration::new(handle, address))
    }
}

impl fmt::Debug for MioHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}

// ========= impl Registration ======== //

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
