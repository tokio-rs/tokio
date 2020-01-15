//! Thread local runtime context
use crate::io::Registration;
use crate::runtime::Handle;
use crate::task::JoinHandle;
use mio::Evented;
use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::net;
use std::vec;

thread_local! {
    static CONTEXT: RefCell<Option<Handle>> = RefCell::new(None)
}

pub(crate) fn current() -> Option<Handle> {
    CONTEXT.with(|ctx| ctx.borrow().clone())
}

pub(crate) fn simulation_handle() -> Option<crate::simulation::SimulationHandle> {
    CONTEXT.with(
        |ctx| match ctx.borrow().as_ref().map(|ctx| ctx.simulation.clone()) {
            Some(Some(simulation)) => Some(simulation),
            _ => None,
        },
    )
}

fn parse_str_addr(
    addr: &str,
    sim: crate::simulation::SimulationHandle,
) -> io::Result<vec::IntoIter<net::SocketAddr>> {
    fn err(msg: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, msg)
    }
    let mut parts_iter = addr.rsplitn(2, ':');
    let port_str = parts_iter.next().ok_or(err("invalid socket address"))?;
    let host = parts_iter.next().ok_or(err("invalid socket address"))?;
    let port: u16 = port_str.parse().map_err(|_| err("invalid port value"))?;
    sim.resolve_tuple(&(host, port))
}

fn sim_resolve_str_addr(addr: &str) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
    if let Some(sim) = simulation_handle() {
        Some(parse_str_addr(addr, sim))
    } else {
        None
    }
}

fn sim_resolve_tuple_addr(
    addr: &(&str, u16),
) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
    if let Some(sim) = simulation_handle() {
        Some(sim.resolve_tuple(addr))
    } else {
        None
    }
}

fn sim_tcp_listener_bind_addr(
    addr: net::SocketAddr,
) -> Option<io::Result<crate::net::tcp::ListenerInner>> {
    if let Some(sim) = simulation_handle() {
        let listener = sim
            .bind(addr.port())
            .map(|s| crate::net::tcp::ListenerInner::Sim(s));
        Some(listener)
    } else {
        None
    }
}

async fn sim_tcp_connect_addr(
    addr: net::SocketAddr,
) -> Option<io::Result<crate::net::tcp::StreamInner>> {
    if let Some(sim) = simulation_handle() {
        let conn = sim.connect(addr).await;
        let conn = conn.map(|c| crate::net::tcp::StreamInner::Sim(c));
        Some(conn)
    } else {
        None
    }
}

cfg_io_driver! {
    fn io_handle() -> crate::runtime::io::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.io_handle.clone(),
            None => Default::default(),
        })
    }

    pub(crate) fn resolve_str_addr(addr: &str) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
        let handle = io_handle();
        handle.map(|h| h.resolve_str_addr(addr)).or_else(|| sim_resolve_str_addr(addr))
    }

    pub(crate) fn resolve_tuple_addr(addr: &(&str, u16)) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
        let handle = io_handle();
        handle.map(|h| h.resolve_tuple_addr(addr)).or_else(|| sim_resolve_tuple_addr(addr))
    }

    pub(crate) fn tcp_listener_bind_addr(addr: net::SocketAddr) -> Option<io::Result<crate::net::tcp::ListenerInner>> {
        let handle = io_handle();
        handle.map(|h| h.tcp_listener_bind_addr(addr)).or_else(|| sim_tcp_listener_bind_addr(addr))
    }

    pub(crate) async fn tcp_stream_connect_addr(addr: net::SocketAddr) -> Option<io::Result<crate::net::tcp::StreamInner>> {
        let handle = io_handle();
        if let Some(h) = handle {
            Some(h.tcp_stream_connect_addr(addr).await)
        } else {
            sim_tcp_connect_addr(addr).await
        }
    }

    pub(crate) fn register_io<T>(io: &T) -> Option<io::Result<Registration>>
    where
        T: Evented, {
            let handle = io_handle();
            handle.map(|h| h.register_io(io))
        }
}

cfg_time! {
    pub(crate) fn time_handle() -> crate::runtime::time::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.time_handle.clone(),
            None => Default::default(),
        })
    }

    cfg_test_util! {
        pub(crate) fn clock() -> Option<crate::runtime::time::Clock> {
            CONTEXT.with(|ctx| match *ctx.borrow() {
                Some(ref ctx) => Some(ctx.clock.clone()),
                None => None,
            })
        }
    }
}

fn simulated_task<T>(task: T) -> crate::simulation::SimTask<T>
where
    T: Future + Send + 'static,
{
    let machineid = crate::simulation::current_machineid();
    crate::simulation::SimTask::new(task, machineid)
}

pub(crate) fn spawn<T>(task: T) -> JoinHandle<T::Output>
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    if simulation_handle().is_some() {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => return ctx.spawner.spawn(simulated_task(task)),
            None => panic!("must be called from the context of Tokio runtime configured with either `basic_scheduler` or `threaded_scheduler`")
        })
    } else {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => return ctx.spawner.spawn(task),
            None => panic!("must be called from the context of Tokio runtime configured with either `basic_scheduler` or `threaded_scheduler`")
        })
    }
}

/// Set this [`ThreadContext`] as the current active [`ThreadContext`].
///
/// [`ThreadContext`]: struct.ThreadContext.html
pub(crate) fn enter<F, R>(new: Handle, f: F) -> R
where
    F: FnOnce() -> R,
{
    struct DropGuard(Option<Handle>);

    impl Drop for DropGuard {
        fn drop(&mut self) {
            CONTEXT.with(|ctx| {
                *ctx.borrow_mut() = self.0.take();
            });
        }
    }

    let _guard = CONTEXT.with(|ctx| {
        let old = ctx.borrow_mut().replace(new);
        DropGuard(old)
    });

    f()
}
