use super::{Context, CONTEXT};

use crate::runtime::{scheduler, TryCurrentError};
use crate::util::rand::{FastRand, RngSeed};

#[derive(Debug)]
#[must_use]
pub(crate) struct SetCurrentGuard {
    old_handle: Option<scheduler::Handle>,
    old_seed: RngSeed,
}

/// Sets this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: crate::runtime::scheduler::Handle
pub(crate) fn try_set_current_guard(handle: &scheduler::Handle) -> Option<SetCurrentGuard> {
    CONTEXT.try_with(|ctx| ctx.set_current_guard(handle)).ok()
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
    pub(super) fn set_current_guard(&self, handle: &scheduler::Handle) -> SetCurrentGuard {
        let rng_seed = handle.seed_generator().next_seed();

        let old_handle = self.handle.borrow_mut().replace(handle.clone());
        let mut rng = self.rng.get().unwrap_or_else(FastRand::new);
        let old_seed = rng.replace_seed(rng_seed);
        self.rng.set(Some(rng));

        SetCurrentGuard {
            old_handle,
            old_seed,
        }
    }
}

impl Drop for SetCurrentGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            *ctx.handle.borrow_mut() = self.old_handle.take();

            let mut rng = ctx.rng.get().unwrap_or_else(FastRand::new);
            rng.replace_seed(self.old_seed.clone());
            ctx.rng.set(Some(rng));
        });
    }
}
