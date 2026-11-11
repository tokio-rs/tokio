//! Running the `current_thread` scheduler from a host event loop: a driver
//! turn that never waits followed by one batch, or a wait that runs no
//! tasks. See `runtime::event_loop`.
//!
//! The host's idle time is the scheduler's park: `parked` is set while the
//! loop is neither ticked nor parked, so that the first remote wakeup of an
//! idle period unparks the driver, which makes the reactor's descriptor
//! readable, and runs `on_work_available`. As in `Context::park`, `parked`
//! is set before the host re-checks for work in `next_sleep`, and both sides
//! swap, so either the wakeup observes `parked` or the host observes the
//! work queued before it.

use super::{Context, Core, CoreGuard, CurrentThread, Handle};
use crate::loom::sync::Arc;

use std::sync::atomic::Ordering::{AcqRel, Acquire};
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
    /// The host is idle: the next remote wakeup notifies it.
    pub(crate) fn set_host_idle(&self) {
        self.shared.parked.swap(true, AcqRel);
    }

    /// Whether a remote wakeup has queued tasks, or an `Unparker` has woken
    /// the loop, since the last tick or park began.
    pub(crate) fn has_remote_work(&self) -> bool {
        !self.shared.inject.is_empty() || self.shared.woken.load(Acquire)
    }

    /// `Unparker::unpark`. It carries no task, so `woken` records it for
    /// `next_sleep` and the next park.
    pub(crate) fn unpark_host(&self) {
        // A swap rather than a store: loom only tracks a total modification
        // order between RMWs (tokio-rs/loom#282), and a turn's own swap of
        // `woken` must be ordered before this write.
        self.shared.woken.swap(true, AcqRel);
        self.unpark();
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
}

/// A tick or park: the host is not idle from `begin` to drop, a hook or
/// waker panicking out of the turn included.
struct Turn<'a> {
    handle: &'a Handle,
    /// An `Unparker::unpark` since the last turn, consumed by this one.
    unparked: bool,
}

impl<'a> Turn<'a> {
    fn begin(handle: &'a Handle) -> Turn<'a> {
        handle.shared.parked.swap(false, AcqRel);
        let unparked = handle.shared.woken.swap(false, AcqRel);
        Turn { handle, unparked }
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.handle.set_host_idle();
    }
}

impl CoreGuard<'_> {
    fn tick(self, max_tasks: usize) -> (usize, bool) {
        let ret = self.enter(|core, context| {
            let handle = &context.handle;
            let _turn = Turn::begin(handle);
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
            let turn = Turn::begin(handle);
            let timeout = if turn.unparked {
                Some(Duration::ZERO)
            } else {
                timeout
            };
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

        // As in `Context::park`: `parked` is set before the re-check, so a
        // remote wakeup either unparks the driver or is seen here. The time
        // driver bounds the wait by its next expiration; ready work bounds
        // it to a zero-timeout turn.
        let wait = timeout != Some(Duration::ZERO) && !self.has_pending_work(&core) && {
            handle.shared.parked.swap(true, AcqRel);
            !self.has_pending_work(&core)
        };

        if wait {
            core.metrics.about_to_park();
            core.submit_metrics(handle);

            core = self.park_internal(core, handle, &mut driver, timeout);

            core.metrics.unparked();
            core.submit_metrics(handle);
        } else {
            core.submit_metrics(handle);

            core = self.park_internal(core, handle, &mut driver, Some(Duration::ZERO));
        }
        handle.shared.parked.swap(false, AcqRel);

        if let Some(f) = &handle.shared.config.after_unpark {
            let (c, ()) = self.enter(core, || f());
            core = c;
        }

        core.driver = Some(driver);
        let local = !core.tasks.is_empty();
        (core, local)
    }
}
