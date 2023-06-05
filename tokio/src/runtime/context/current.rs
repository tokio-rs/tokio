use super::{Context, CONTEXT};

use crate::runtime::{scheduler, TryCurrentError};
use crate::util::markers::SyncNotSend;

use std::marker::PhantomData;

#[derive(Debug)]
#[must_use]
pub(crate) struct SetCurrentGuard {
    old_handle: Option<scheduler::Handle>,
    _p: PhantomData<SyncNotSend>,
}

/// Sets this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: crate::runtime::scheduler::Handle
pub(crate) fn try_set_current(handle: &scheduler::Handle) -> Option<SetCurrentGuard> {
    CONTEXT
        .try_with(|ctx| {
            let old_handle = ctx.handle.borrow_mut().replace(handle.clone());

            SetCurrentGuard {
                old_handle,
                _p: PhantomData,
            }
        })
        .ok()
}

pub(crate) fn with_current<F, R>(f: F) -> Result<R, TryCurrentError>
where
    F: FnOnce(&scheduler::Handle) -> R,
{
    match CONTEXT.try_with(|ctx| ctx.handle.borrow().as_ref().map(f)) {
        Ok(Some(ret)) => Ok(ret),
        Ok(None) => Err(TryCurrentError::new_no_context()),
        Err(_access_error) => Err(TryCurrentError::new_thread_local_destroyed()),
    }
}

impl Context {
    pub(super) fn set_current(&self, handle: &scheduler::Handle) -> SetCurrentGuard {
        let old_handle = self.handle.borrow_mut().replace(handle.clone());

        SetCurrentGuard {
            old_handle,
            _p: PhantomData,
        }
    }
}

impl Drop for SetCurrentGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            *ctx.handle.borrow_mut() = self.old_handle.take();
        });
    }
}
