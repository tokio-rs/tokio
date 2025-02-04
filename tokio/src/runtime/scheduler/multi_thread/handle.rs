use crate::future::Future;
use crate::loom::sync::Arc;
#[cfg(tokio_unstable)]
use crate::runtime::context::with_task_hooks;
use crate::runtime::scheduler::multi_thread::worker;
#[cfg(tokio_unstable)]
use crate::runtime::task::Schedule;
use crate::runtime::{
    blocking, driver,
    task::{self, JoinHandle},
};
#[cfg(tokio_unstable)]
use crate::runtime::{OnChildTaskSpawnContext, OnTopLevelTaskSpawnContext, OptionalTaskHooks};
use crate::util::RngSeedGenerator;
use std::fmt;
#[cfg(tokio_unstable)]
use std::panic;

mod metrics;

cfg_taskdump! {
    mod taskdump;
}

/// Handle to the multi thread scheduler
pub(crate) struct Handle {
    /// Task spawner
    pub(super) shared: worker::Shared,

    /// Resource driver handles
    pub(crate) driver: driver::Handle,

    /// Blocking pool spawner
    pub(crate) blocking_spawner: blocking::Spawner,

    /// Current random number generator seed
    pub(crate) seed_generator: RngSeedGenerator,
}

impl Handle {
    /// Spawns a future onto the thread pool
    pub(crate) fn spawn<F>(
        me: &Arc<Self>,
        future: F,
        id: task::Id,
        #[cfg(tokio_unstable)] hooks_override: OptionalTaskHooks,
    ) -> JoinHandle<F::Output>
    where
        F: crate::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        #[cfg(tokio_unstable)]
        return Self::bind_new_task(me, future, id, hooks_override);

        #[cfg(not(tokio_unstable))]
        Self::bind_new_task(me, future, id)
    }

    pub(crate) fn shutdown(&self) {
        self.close();
    }

    pub(super) fn bind_new_task<T>(
        me: &Arc<Self>,
        future: T,
        id: task::Id,
        #[cfg(tokio_unstable)] hooks_override: OptionalTaskHooks,
    ) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        // preference order for hook selection:
        // 1. "hook override" - comes from builder
        // 2. parent task's hook
        // 3. runtime hook factory
        #[cfg(tokio_unstable)]
        let hooks = hooks_override.or_else(|| {
            with_task_hooks(|parent| {
                parent
                    .map(|parent| {
                        if let Ok(r) = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                            parent.on_child_spawn(&mut OnChildTaskSpawnContext {
                                id,
                                _phantom: Default::default(),
                            })
                        })) {
                            r
                        } else {
                            None
                        }
                    })
                    .flatten()
            })
            .ok()
            .flatten()
            .or_else(|| {
                if let Some(hooks) = me.hooks_factory_ref() {
                    if let Ok(r) = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        hooks.on_top_level_spawn(&mut OnTopLevelTaskSpawnContext {
                            id,
                            _phantom: Default::default(),
                        })
                    })) {
                        r
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        });

        let (handle, notified) = me.shared.owned.bind(
            future,
            me.clone(),
            id,
            #[cfg(tokio_unstable)]
            hooks,
        );

        me.schedule_option_task_without_yield(notified);

        handle
    }
}

cfg_unstable! {
    use std::num::NonZeroU64;

    impl Handle {
        pub(crate) fn owned_id(&self) -> NonZeroU64 {
            self.shared.owned.id
        }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("multi_thread::Handle { ... }").finish()
    }
}
