mod mio_driver;

pub(crate) mod platform;
mod scheduled_io;

pub(crate) use mio_driver::Driver;
pub(crate) use scheduled_io::ScheduledIo; // pub(crate) for tests

use crate::future::poll_fn;
use crate::io::PollEvented;
use crate::net::tcp::{listener::ListenerInner, stream::StreamInner};
use mio::event::Evented;

pub use mio_driver::Registration;
use std::{io, net, vec};

#[derive(Debug, Clone)]
pub(crate) enum Handle {
    Mio(mio_driver::Handle),
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
    pub(crate) fn register_io<T>(&self, io: &T) -> io::Result<Registration>
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
