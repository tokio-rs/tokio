use super::{BlockingRegionGuard, SetCurrentGuard, CONTEXT};

use crate::runtime::scheduler;
use crate::util::rand::{FastRand, RngSeed};

use std::fmt;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) enum EnterRuntime {
    /// Currently in a runtime context.
    #[cfg_attr(not(feature = "rt"), allow(dead_code))]
    Entered { allow_block_in_place: bool },

    /// Not in a runtime context **or** a blocking region.
    NotEntered,
}

/// Guard tracking that a caller has entered a runtime context.
#[must_use]
pub(crate) struct EnterRuntimeGuard {
    /// Tracks that the current thread has entered a blocking function call.
    pub(crate) blocking: BlockingRegionGuard,

    #[allow(dead_code)] // Only tracking the guard.
    pub(crate) handle: SetCurrentGuard,

    // Tracks the previous random number generator seed
    old_seed: RngSeed,
}

/// Marks the current thread as being within the dynamic extent of an
/// executor.
#[track_caller]
pub(crate) fn enter_runtime<F, R>(handle: &scheduler::Handle, allow_block_in_place: bool, f: F) -> R
where
    F: FnOnce(&mut BlockingRegionGuard) -> R,
{
    let rng_seed = || handle.seed_generator().next_seed();
    match try_enter_runtime(handle, allow_block_in_place, rng_seed) {
        Some(mut guard) => f(&mut guard.blocking),
        None => already_entered(),
    }
}

/// [`enter_runtime`] continuing the random number generator from `seed`, and
/// leaving `seed` where the call's draws advanced it, rather than drawing a
/// seed from the runtime's generator, which takes a lock. A host event loop
/// enters the runtime for each of its turns, and its turns share one stream
/// as the iterations of a `block_on` do.
#[track_caller]
#[cfg(all(feature = "rt", tokio_unstable))]
pub(crate) fn enter_runtime_with_seed<F, R>(
    handle: &scheduler::Handle,
    allow_block_in_place: bool,
    seed: &mut RngSeed,
    f: F,
) -> R
where
    F: FnOnce(&mut BlockingRegionGuard) -> R,
{
    let Some(mut guard) = try_enter_runtime(handle, allow_block_in_place, || seed.clone()) else {
        already_entered()
    };
    let ret = f(&mut guard.blocking);
    if let Some(advanced) = CONTEXT.with(|c| c.rng.get()).map(|mut rng| rng.replace_seed(guard.old_seed.clone())) {
        *seed = advanced;
    }
    ret
}

#[track_caller]
fn try_enter_runtime(
    handle: &scheduler::Handle,
    allow_block_in_place: bool,
    rng_seed: impl FnOnce() -> RngSeed,
) -> Option<EnterRuntimeGuard> {
    CONTEXT.with(|c| {
        if c.runtime.get().is_entered() {
            None
        } else {
            // Set the entered flag
            c.runtime.set(EnterRuntime::Entered {
                allow_block_in_place,
            });

            let rng_seed = rng_seed();

            // Swap the RNG seed
            let mut rng = c.rng.get().unwrap_or_else(FastRand::new);
            let old_seed = rng.replace_seed(rng_seed);
            c.rng.set(Some(rng));

            Some(EnterRuntimeGuard {
                blocking: BlockingRegionGuard::new(),
                handle: c.set_current(handle),
                old_seed,
            })
        }
    })
}

#[cold]
#[track_caller]
fn already_entered() -> ! {
    panic!(
        "Cannot start a runtime from within a runtime. This happens \
            because a function (like `block_on`) attempted to block the \
            current thread while the thread is being used to drive \
            asynchronous tasks. If you are in an async function, use \
            `.await` on the future instead of blocking on it."
    );
}

impl fmt::Debug for EnterRuntimeGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Enter").finish()
    }
}

impl Drop for EnterRuntimeGuard {
    fn drop(&mut self) {
        CONTEXT.with(|c| {
            assert!(c.runtime.get().is_entered());
            c.runtime.set(EnterRuntime::NotEntered);
            // Replace the previous RNG seed
            let mut rng = c.rng.get().unwrap_or_else(FastRand::new);
            rng.replace_seed(self.old_seed.clone());
            c.rng.set(Some(rng));
        });
    }
}

impl EnterRuntime {
    pub(crate) fn is_entered(self) -> bool {
        matches!(self, EnterRuntime::Entered { .. })
    }
}
