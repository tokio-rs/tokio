//! Thread local runtime context
use crate::io::Registration;
use crate::runtime::Handle;
use mio::Evented;
use std::cell::RefCell;
use std::io;
use std::net;
use std::vec;

thread_local! {
    static CONTEXT: RefCell<Option<Handle>> = RefCell::new(None)
}

pub(crate) fn current() -> Option<Handle> {
    CONTEXT.with(|ctx| ctx.borrow().clone())
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
        handle.map(|h| h.resolve_str_addr(addr))
    }

    pub(crate) fn resolve_tuple_addr(addr: &(&str, u16)) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
        let handle = io_handle();
        handle.map(|h| h.resolve_tuple_addr(addr))
    }

    pub(crate) fn tcp_listener_bind_addr(addr: net::SocketAddr) -> Option<io::Result<crate::net::tcp::ListenerInner>> {
        let handle = io_handle();
        handle.map(|h| h.tcp_listener_bind_addr(addr))
    }

    pub(crate) async fn tcp_stream_connect_addr(addr: net::SocketAddr) -> Option<io::Result<crate::net::tcp::StreamInner>> {
        let handle = io_handle();
        if let Some(h) = handle {
            Some(h.tcp_stream_connect_addr(addr).await)
        } else {
            None
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

cfg_rt_core! {
    pub(crate) fn spawn_handle() -> Option<crate::runtime::Spawner> {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => Some(ctx.spawner.clone()),
            None => None,
        })
    }

    pub(crate) fn simulation_handle() -> Option<crate::simulation::SimulationHandle> {
        CONTEXT.with(
            |ctx| match ctx.borrow().as_ref().map(|ctx| ctx.simulation.clone()) {
                Some(Some(simulation)) => Some(simulation),
                _ => None
            }
        )
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
