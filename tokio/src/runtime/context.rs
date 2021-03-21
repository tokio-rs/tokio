//! Thread local runtime context
use crate::runtime::Handle;

use std::cell::RefCell;

thread_local! {
    static CONTEXT: RefCell<Option<Handle>> = RefCell::new(None)
}

pub(crate) fn current() -> Option<Handle> {
    CONTEXT.with(|ctx| ctx.borrow().clone())
}

cfg_io_driver! {
    pub(crate) fn io_handle() -> crate::runtime::driver::IoHandle {
        CONTEXT.with(|ctx| {
            let ctx = ctx.borrow();
            ctx.as_ref().expect(crate::util::error::CONTEXT_MISSING_ERROR).io_handle.clone()
        })
    }
}

cfg_signal_internal! {
    #[cfg(unix)]
    pub(crate) fn signal_handle() -> crate::runtime::driver::SignalHandle {
        CONTEXT.with(|ctx| {
            let ctx = ctx.borrow();
            ctx.as_ref().expect(crate::util::error::CONTEXT_MISSING_ERROR).signal_handle.clone()
        })
    }
}

cfg_time! {
    pub(crate) fn time_handle() -> crate::runtime::driver::TimeHandle {
        CONTEXT.with(|ctx| {
            let ctx = ctx.borrow();
            ctx.as_ref().expect(crate::util::error::CONTEXT_MISSING_ERROR).time_handle.clone()
        })
    }

    cfg_test_util! {
        pub(crate) fn clock() -> Option<crate::runtime::driver::Clock> {
            CONTEXT.with(|ctx| (*ctx.borrow()).as_ref().map(|ctx| ctx.clock.clone()))
        }
    }
}

cfg_rt! {
    pub(crate) fn spawn_handle() -> Option<crate::runtime::Spawner> {
        CONTEXT.with(|ctx| (*ctx.borrow()).as_ref().map(|ctx| ctx.spawner.clone()))
    }
}

/// Set this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: Handle
pub(crate) fn enter(new: Handle) -> EnterGuard {
    CONTEXT.with(|ctx| {
        let old = ctx.borrow_mut().replace(new);
        EnterGuard(old)
    })
}

#[derive(Debug)]
pub(crate) struct EnterGuard(Option<Handle>);

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = self.0.take();
        });
    }
}
