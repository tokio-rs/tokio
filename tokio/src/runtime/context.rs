//! Thread local runtime context
use crate::runtime::task::Id;
use crate::runtime::{Handle, TryCurrentError};
use crate::util::{replace_thread_rng, RngSeed};

use std::cell::{Cell, RefCell};

struct Context {
    handle: RefCell<Option<Handle>>,
    task_id: Cell<Option<Id>>,
}

thread_local! {
    static CONTEXT: Context = const { Context {
        handle: RefCell::new(None),
        task_id: Cell::new(None),
    }}
}

pub(crate) fn try_current() -> Result<Handle, crate::runtime::TryCurrentError> {
    match CONTEXT.try_with(|ctx| ctx.handle.borrow().clone()) {
        Ok(Some(handle)) => Ok(handle),
        Ok(None) => Err(TryCurrentError::new_no_context()),
        Err(_access_error) => Err(TryCurrentError::new_thread_local_destroyed()),
    }
}

#[track_caller]
pub(crate) fn current() -> Handle {
    match try_current() {
        Ok(handle) => handle,
        Err(e) => panic!("{}", e),
    }
}

pub(crate) fn set_current_task_id(id: Option<Id>) -> Option<Id> {
    CONTEXT.with(|ctxt| ctxt.task_id.replace(id))
}

pub(crate) fn current_task_id() -> Id {
    match CONTEXT.with(|ctxt| ctxt.task_id.get()) {
        Some(id) => id,
        _ => panic!("tried to get current task id from outside of task"),
    }
}

cfg_io_driver! {
    #[track_caller]
    pub(crate) fn io_handle() -> crate::runtime::driver::IoHandle {
        match CONTEXT.try_with(|ctx| {
            let ctx = ctx.handle.borrow();
            ctx.as_ref()
                .expect(crate::util::error::CONTEXT_MISSING_ERROR)
                .inner
                .driver()
                .io
                .clone()
        }) {
            Ok(io_handle) => io_handle,
            Err(_) => panic!("{}", crate::util::error::THREAD_LOCAL_DESTROYED_ERROR),
        }
    }
}

cfg_signal_internal! {
    #[cfg(unix)]
    pub(crate) fn signal_handle() -> crate::runtime::driver::SignalHandle {
        match CONTEXT.try_with(|ctx| {
            let ctx = ctx.handle.borrow();
            ctx.as_ref()
                .expect(crate::util::error::CONTEXT_MISSING_ERROR)
                .inner
                .signal()
                .clone()
        }) {
            Ok(signal_handle) => signal_handle,
            Err(_) => panic!("{}", crate::util::error::THREAD_LOCAL_DESTROYED_ERROR),
        }
    }
}

cfg_time! {
    cfg_test_util! {
        pub(crate) fn clock() -> Option<crate::runtime::driver::Clock> {
            match CONTEXT.try_with(|ctx| {
                let ctx = ctx.handle.borrow();
                ctx
                    .as_ref()
                    .map(|ctx| ctx.inner.clock().clone())
            }) {
                Ok(clock) => clock,
                Err(_) => panic!("{}", crate::util::error::THREAD_LOCAL_DESTROYED_ERROR),
            }
        }
    }
}

/// Sets this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: Handle
pub(crate) fn enter(new: Handle) -> EnterGuard {
    match try_enter(new) {
        Some(guard) => guard,
        None => panic!("{}", crate::util::error::THREAD_LOCAL_DESTROYED_ERROR),
    }
}

/// Sets this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: Handle
pub(crate) fn try_enter(new: Handle) -> Option<EnterGuard> {
    let rng_seed = new.inner.seed_generator().next_seed();
    let old_handle = CONTEXT.try_with(|ctx| ctx.handle.borrow_mut().replace(new)).ok()?;

    let old_seed = replace_thread_rng(rng_seed);

    Some(EnterGuard {
        old_handle,
        old_seed,
    })
}

#[derive(Debug)]
pub(crate) struct EnterGuard {
    old_handle: Option<Handle>,
    old_seed: RngSeed,
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            *ctx.handle.borrow_mut() = self.old_handle.take();
        });
        // We discard the RngSeed associated with this guard
        let _ = replace_thread_rng(self.old_seed.clone());
    }
}
