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

    /// Blocking pool spawner
    blocking_spawner: Option<crate::runtime::blocking::Spawner>,
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
            blocking_spawner: None,
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
        blocking_spawner: Option<crate::runtime::blocking::Spawner>,
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
            blocking_spawner,
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

    pub(crate) fn io_handle() -> crate::runtime::io::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.io_handle.clone(),
            None => Default::default(),
        })
    }

    pub(crate) fn time_handle() -> crate::runtime::time::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.time_handle.clone(),
            None => Default::default(),
        })
    }

    pub(crate) fn spawn_handle() -> Option<Spawner> {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => Some(ctx.spawner.clone()),
            None => None,
        })
    }

    pub(crate) fn clock() -> Option<crate::runtime::time::Clock> {
        CONTEXT.with(
            |ctx| match ctx.borrow().as_ref().map(|ctx| ctx.clock.clone()) {
                Some(Some(clock)) => Some(clock),
                _ => None,
            },
        )
    }

    pub(crate) fn blocking_spawner() -> Option<crate::runtime::blocking::Spawner> {
        CONTEXT.with(|ctx| {
            match ctx
                .borrow()
                .as_ref()
                .map(|ctx| ctx.blocking_spawner.clone())
            {
                Some(Some(blocking_spawner)) => Some(blocking_spawner),
                _ => None,
            }
        })
    }
}

cfg_blocking_impl! {
    impl ThreadContext {
        pub(crate) fn with_blocking_spawner(
            mut self,
            blocking_spawner: crate::runtime::blocking::Spawner,
        ) -> Self {
            self.blocking_spawner.replace(blocking_spawner);
            self
        }
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
