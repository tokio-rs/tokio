//! Global runtime context
#![allow(dead_code)]

use crate::runtime::{time, Spawner};

use std::cell::RefCell;

thread_local! {
    static CONTEXT: RefCell<Option<ThreadContext>> = RefCell::new(None)
}

/// ThreadContext makes Runtime context accessible to each Runtime thread.
#[derive(Debug, Clone)]
pub(crate) struct ThreadContext {
    executing: bool,
    /// Handles to the executor.
    spawner: Spawner,

    /// Handles to the I/O drivers
    io_handle: crate::runtime::io::Handle,

    /// Handles to the time drivers
    time_handle: crate::runtime::time::Handle,

    /// Source of `Instant::now()`
    clock: Option<crate::runtime::time::Clock>,
}

impl ThreadContext {
    pub(crate) fn new() -> Self {
        ThreadContext {
            executing: false,
            spawner: Spawner::Shell,
            io_handle: None,
            time_handle: None,
            clock: None,
        }
    }

    pub(crate) fn clone_current() -> Self {
        CONTEXT.with(|ctx| ctx.borrow().clone().unwrap_or(ThreadContext::new()))
    }

    pub(crate) fn with_spawner(mut self, spawner: Spawner) -> Self {
        self.spawner = spawner;
        self
    }

    pub(crate) fn with_io_handle(mut self, handle: crate::runtime::io::Handle) -> Self {
        self.io_handle = handle;
        self
    }

    pub(crate) fn with_time_handle(mut self, handle: crate::runtime::time::Handle) -> Self {
        self.time_handle = handle;
        self
    }

    pub(crate) fn with_clock(mut self, clock: time::Clock) -> Self {
        self.clock.replace(clock);
        self
    }

    /// Set this ThreadContext as the thread context for the runtime thread.
    pub(crate) fn enter(self) -> ThreadContextDropGuard {
        CONTEXT.with(|ctx| {
            let previous = ctx.replace(Some(self));
            ThreadContextDropGuard { previous }
        })
    }

    pub(crate) fn take_current() -> Option<ThreadContext> {
        CONTEXT.with(|ctx| ctx.borrow_mut().take())
    }

    pub(crate) fn entered() -> bool {
        CONTEXT.with(|ctx| (*ctx.borrow()).is_some())
    }

    pub(crate) fn io_handle() -> crate::runtime::io::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.io_handle.clone(),
            None => None,
        })
    }

    pub(crate) fn time_handle() -> crate::runtime::time::Handle {
        CONTEXT.with(|ctx| match *ctx.borrow() {
            Some(ref ctx) => ctx.time_handle.clone(),
            None => None,
        })
    }

    pub(crate) fn clock() -> Option<crate::runtime::time::Clock> {
        CONTEXT
            .with(|ctx| ctx.borrow().as_ref().map(|ctx| ctx.clock.clone()))
            .flatten()
    }
}

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
