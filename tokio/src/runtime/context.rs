use crate::util::rand::{FastRand, RngSeed};

cfg_rt! {
    use crate::runtime::scheduler;
    use std::cell::RefCell;
}

struct Context {
    /// Handle to the runtime scheduler running on the current thread.
    #[cfg(feature = "rt")]
    scheduler: RefCell<Option<scheduler::Handle>>,
    rng: FastRand,
}

tokio_thread_local! {
    static CONTEXT: Context = {
        Context {
            #[cfg(feature = "rt")]
            scheduler: RefCell::new(None),
            rng: FastRand::new(RngSeed::new()),
        }
    }
}

#[cfg(feature = "macros")]
pub(crate) fn thread_rng_n(n: u32) -> u32 {
    CONTEXT.with(|ctx| ctx.rng.fastrand_n(n))
}

cfg_rt! {
    use crate::runtime::TryCurrentError;

    #[derive(Debug)]
    pub(crate) struct SetCurrentGuard {
        old_handle: Option<scheduler::Handle>,
        old_seed: RngSeed,
    }

    pub(crate) fn try_current() -> Result<scheduler::Handle, TryCurrentError> {
        match CONTEXT.try_with(|ctx| ctx.scheduler.borrow().clone()) {
            Ok(Some(handle)) => Ok(handle),
            Ok(None) => Err(TryCurrentError::new_no_context()),
            Err(_access_error) => Err(TryCurrentError::new_thread_local_destroyed()),
        }
    }

    /// Sets this [`Handle`] as the current active [`Handle`].
    ///
    /// [`Handle`]: crate::runtime::scheduler::Handle
    pub(crate) fn try_set_current(handle: &scheduler::Handle) -> Option<SetCurrentGuard> {
        let rng_seed = handle.seed_generator().next_seed();

        CONTEXT.try_with(|ctx| {
            let old_handle = ctx.scheduler.borrow_mut().replace(handle.clone());
            let old_seed = ctx.rng.replace_seed(rng_seed);

            SetCurrentGuard {
                old_handle,
                old_seed,
            }
        }).ok()
    }

    impl Drop for SetCurrentGuard {
        fn drop(&mut self) {
            CONTEXT.with(|ctx| {
                *ctx.scheduler.borrow_mut() = self.old_handle.take();
                ctx.rng.replace_seed(self.old_seed.clone());
            });
        }
    }
}
