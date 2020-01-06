//! Thread local runtime context
use crate::runtime::Spawner;
use std::cell::RefCell;

thread_local! {
    static CONTEXT: RefCell<Option<ThreadContext>> = RefCell::new(None)
}

/// ThreadContext makes Runtime context accessible to each Runtime thread.
#[derive(Debug, Clone)]
pub(crate) struct ThreadContext {
    /// Handles to the executor.
    spawner: Spawner,

    /// Handles to the I/O drivers
    io_handle: crate::runtime::io::Handle,

    /// Handles to the time drivers
    time_handle: crate::runtime::time::Handle,

    /// Source of `Instant::now()`
    clock: Option<crate::runtime::time::Clock>,
}

impl Default for ThreadContext {
    fn default() -> Self {
        ThreadContext {
            spawner: Spawner::Shell,
            #[cfg(all(feature = "io-driver", not(loom)))]
            io_handle: None,
            #[cfg(any(not(feature = "io-driver"), loom))]
            io_handle: (),
            #[cfg(all(feature = "time", not(loom)))]
            time_handle: None,
            #[cfg(any(not(feature = "time"), loom))]
            time_handle: (),
            clock: None,
        }
    }
}

impl ThreadContext {
    /// Construct a new [`ThreadContext`]
    ///
    /// [`ThreadContext`]: struct.ThreadContext.html
    pub(crate) fn new(
        spawner: Spawner,
        io_handle: crate::runtime::io::Handle,
        time_handle: crate::runtime::time::Handle,
        clock: Option<crate::runtime::time::Clock>,
    ) -> Self {
        ThreadContext {
            spawner,
            #[cfg(all(feature = "io-driver", not(loom)))]
            io_handle,
            #[cfg(any(not(feature = "io-driver"), loom))]
            io_handle,
            #[cfg(all(feature = "time", not(loom)))]
            time_handle,
            #[cfg(any(not(feature = "time"), loom))]
            time_handle,
            clock,
        }
    }

    /// Clone the current [`ThreadContext`] if one is set, otherwise construct a new [`ThreadContext`].
    ///
    /// [`ThreadContext`]: struct.ThreadContext.html
    #[allow(dead_code)]
    pub(crate) fn clone_current() -> Self {
        CONTEXT.with(|ctx| ctx.borrow().clone().unwrap_or_else(Default::default))
    }

    /// Set this [`ThreadContext`] as the current active [`ThreadContext`].
    ///
    /// [`ThreadContext`]: struct.ThreadContext.html
    pub(crate) fn enter(self) -> ThreadContextDropGuard {
        CONTEXT.with(|ctx| {
            let previous = ctx.borrow_mut().replace(self);
            ThreadContextDropGuard { previous }
        })
    }

    #[cfg(all(feature = "io-driver", not(loom)))]
    pub(crate) fn io_handle() -> crate::runtime::io::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.io_handle.clone(),
            None => None,
        })
    }

    #[cfg(all(feature = "time", not(loom)))]
    pub(crate) fn time_handle() -> crate::runtime::time::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.time_handle.clone(),
            None => None,
        })
    }

    #[cfg(feature = "rt-core")]
    pub(crate) fn spawn_handle() -> Option<Spawner> {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => Some(ctx.spawner.clone()),
            None => None,
        })
    }

    #[cfg(all(feature = "test-util", feature = "time"))]
    pub(crate) fn clock() -> Option<crate::runtime::time::Clock> {
        CONTEXT.with(
            |ctx| match ctx.borrow().as_ref().map(|ctx| ctx.clock.clone()) {
                Some(Some(clock)) => Some(clock),
                _ => None,
            },
        )
    }
}

/// [`ThreadContextDropGuard`] will replace the `previous` thread context on drop.
///
/// [`ThreadContextDropGuard`]: struct.ThreadContextDropGuard.html
#[derive(Debug)]
pub(crate) struct ThreadContextDropGuard {
    previous: Option<ThreadContext>,
}

impl Drop for ThreadContextDropGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| match self.previous.clone() {
            Some(prev) => ctx.borrow_mut().replace(prev),
            None => ctx.borrow_mut().take(),
        });
    }
}
