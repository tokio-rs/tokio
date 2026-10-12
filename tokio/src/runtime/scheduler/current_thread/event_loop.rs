//! Running the `current_thread` scheduler from a host event loop: a driver
//! turn that never waits followed by one batch, or a wait that runs no
//! tasks. See `runtime::event_loop`.

use super::{Context, Core, CoreGuard, CurrentThread, Handle};
use crate::loom::sync::Arc;
use crate::runtime::driver;

use std::thread;
use std::time::Duration;

impl CurrentThread {
    /// One zero-timeout driver turn, then up to `max_tasks` tasks. Returns
    /// how many ran and whether tasks remain in the local queue. Must be
    /// called inside `enter_runtime`.
    pub(crate) fn tick(&self, handle: &Arc<Handle>, max_tasks: usize) -> (usize, bool) {
        self.host_core(handle).tick(max_tasks)
    }

    /// Waits in the driver for a wakeup, a timer, or `timeout`, without
    /// running tasks; returns immediately if work is already ready. Returns
    /// whether tasks are in the local queue. Must be called inside
    /// `enter_runtime`.
    pub(crate) fn park(&self, handle: &Arc<Handle>, timeout: Option<Duration>) -> bool {
        self.host_core(handle).park(timeout)
    }

    fn host_core(&self, handle: &Arc<Handle>) -> CoreGuard<'_> {
        // `enter_runtime` has rejected a nested tick already.
        let core = self.take_core(handle).expect("core checked out");
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core
    }
}

impl Handle {
    /// Whether tasks scheduled from outside the loop are queued.
    pub(crate) fn has_injected_work(&self) -> bool {
        !self.shared.inject.is_empty()
    }

    /// The builder's `event_interval`: tasks per batch.
    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
    pub(crate) fn event_interval(&self) -> usize {
        self.shared.config.event_interval as usize
    }
}

/// A task panicked and the runtime is configured to shut down.
struct Panicked;

impl Context {
    /// Runs up to `max_tasks` tasks.
    fn run_batch(
        &self,
        mut core: Box<Core>,
        max_tasks: usize,
    ) -> (Box<Core>, Result<usize, Panicked>) {
        let handle = &self.handle;
        core.metrics.start_processing_scheduled_tasks();

        let mut ran = 0;
        while ran < max_tasks && !core.unhandled_panic {
            core.tick();
            let Some(task) = core.next_task(handle) else {
                break;
            };
            let task = handle.shared.owned.assert_owner(task);
            core = self.run_task(task, core);
            ran += 1;
        }

        core.metrics.end_processing_scheduled_tasks();
        if core.unhandled_panic {
            return (core, Err(Panicked));
        }
        // Deferred wakers (`yield_now`) are released where a park would; the
        // core must be in the context for their schedules to reach it.
        let (core, ()) = self.enter(core, || self.defer.wake());
        (core, Ok(ran))
    }

    fn has_ready_work(&self, core: &Core) -> bool {
        self.has_pending_work(core) || self.handle.has_injected_work()
    }
}

/// The host's callback is held back from `begin_turn` to drop, a hook or
/// waker panicking out of the turn included.
struct Turn<'a>(&'a driver::Handle);

impl<'a> Turn<'a> {
    fn begin(driver: &'a driver::Handle) -> Turn<'a> {
        driver.begin_turn();
        Turn(driver)
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.end_turn();
    }
}

impl CoreGuard<'_> {
    fn tick(self, max_tasks: usize) -> (usize, bool) {
        let ret = self.enter(|core, context| {
            let handle = &context.handle;
            let _turn = Turn::begin(&handle.driver);
            // The driver's turn: I/O readiness and due timers, ahead of the
            // batch that consumes what they wake.
            let core = context.park_yield(core, handle);
            let (core, ran) = context.run_batch(core, max_tasks);
            let local = !core.tasks.is_empty();
            (core, ran.map(|ran| (ran, local)))
        });

        match ret {
            Ok(ret) => ret,
            Err(Panicked) => panic!(
                "a spawned task panicked and the runtime is configured to shut down on unhandled panic"
            ),
        }
    }

    fn park(self, timeout: Option<Duration>) -> bool {
        self.enter(|core, context| {
            let handle = &context.handle;
            let _turn = Turn::begin(&handle.driver);
            context.park_timeout(core, handle, timeout)
        })
    }
}

impl Context {
    /// `Context::park` with a bound, returning whether tasks are in the
    /// local queue.
    fn park_timeout(
        &self,
        mut core: Box<Core>,
        handle: &Handle,
        timeout: Option<Duration>,
    ) -> (Box<Core>, bool) {
        let mut driver = core.driver.take().expect("driver missing");

        if let Some(f) = &handle.shared.config.before_park {
            let (c, ()) = self.enter(core, || f());
            core = c;
        }

        let idle = !self.has_ready_work(&core);
        if idle {
            core.metrics.about_to_park();
        }
        core.submit_metrics(handle);

        // The time driver bounds the wait by its next expiration; ready work
        // bounds it to a zero-timeout turn.
        let timeout = if idle { timeout } else { Some(Duration::ZERO) };
        core = self.park_internal(core, handle, &mut driver, timeout);

        if idle {
            core.metrics.unparked();
            core.submit_metrics(handle);
        }

        if let Some(f) = &handle.shared.config.after_unpark {
            let (c, ()) = self.enter(core, || f());
            core = c;
        }

        core.driver = Some(driver);
        let local = !core.tasks.is_empty();
        (core, local)
    }
}
