//! Thread local runtime context
use crate::runtime::{Handle, TryCurrentError};
use crate::util::{replace_thread_rng, RngSeed};

use std::cell::RefCell;

tokio_thread_local! {
    static CONTEXT: RefCell<Option<Handle>> = const { RefCell::new(None) }
}

pub(crate) fn try_current() -> Result<Handle, crate::runtime::TryCurrentError> {
    match CONTEXT.try_with(|ctx| ctx.borrow().clone()) {
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
    let old_handle = CONTEXT.try_with(|ctx| ctx.borrow_mut().replace(new)).ok()?;

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
            *ctx.borrow_mut() = self.old_handle.take();
        });
        // We discard the RngSeed associated with this guard
        let _ = replace_thread_rng(self.old_seed.clone());
    }
}
