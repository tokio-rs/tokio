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

trait Syscalls {
    fn resolve_str_addr(&self, addr: &str) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>>;

    fn resolve_tuple_addr(
        &self,
        addr: &(&str, u16),
    ) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>>;

    fn tcp_listener_bind_addr(
        &self,
        addr: net::SocketAddr,
    ) -> Option<io::Result<crate::net::tcp::ListenerInner>>;

    fn tcp_stream_connect_addr(
        &self,
        addr: net::SocketAddr,
    ) -> Option<Box<dyn Future<Output = io::Result<crate::net::tcp::StreamInner>>>>;

    fn register_io(&self, io: Box<dyn Evented>) -> Option<io::Result<Registration>>;
}

impl Syscalls for Handle {
    fn resolve_str_addr(&self, addr: &str) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
        self.io_handle.as_ref().map(|h| h.resolve_str_addr(addr))
    }

    fn resolve_tuple_addr(
        &self,
        addr: &(&str, u16),
    ) -> Option<io::Result<vec::IntoIter<net::SocketAddr>>> {
        self.io_handle.as_ref().map(|h| h.resolve_tuple_addr(addr))
    }

    fn tcp_listener_bind_addr(
        &self,
        addr: net::SocketAddr,
    ) -> Option<io::Result<crate::net::tcp::ListenerInner>> {
        self.io_handle
            .as_ref()
            .map(|h| h.tcp_listener_bind_addr(addr))
    }

    fn tcp_stream_connect_addr(
        &self,
        addr: net::SocketAddr,
    ) -> Option<Box<dyn Future<Output = io::Result<crate::net::tcp::StreamInner>>>> {
        if let Some(io_handle) = self.io_handle.clone().take() {
            Some(Box::new(async move {
                io_handle.tcp_stream_connect_addr(addr).await
            }))
        } else {
            None
        }
    }

    fn register_io(&self, io: Box<dyn Evented>) -> Option<io::Result<Registration>> {
        self.io_handle.as_ref().map(|h| h.register_io(&io))
    }
}

thread_local! {
    static CONTEXTNEW: RefCell<Option<Box<dyn Syscalls>>> = RefCell::new(None)
}

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
    pub(crate) fn simulation_handle() -> Option<crate::simulation::SimulationHandle> {
        CONTEXT.with(
            |ctx| match ctx.borrow().as_ref().map(|ctx| ctx.simulation.clone()) {
                Some(Some(simulation)) => Some(simulation),
                _ => None
            }
        )
    }

    pub(crate) fn spawn<T>(task: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.spawner.spawn(task),
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
