//! Running the `current_thread` scheduler from a host event loop: a driver
//! turn that never waits, followed by one batch. See `runtime::event_loop`.

use super::{Context, Core, CoreGuard, CurrentThread, Handle};
use crate::loom::sync::Arc;

use std::thread;

impl CurrentThread {
    /// One zero-timeout driver turn, then up to `event_interval` tasks.
    /// Returns whether ready work remains queued. Must be called inside
    /// `enter_runtime`.
    pub(crate) fn drive(&self, handle: &Arc<Handle>) -> bool {
        // Nested entry and foreign threads are rejected before this point.
        let core = self.take_core(handle).expect("core checked out");
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core.drive()
    }
}

impl Core {
    /// Ready work in either queue. A batch ending by `Interval` can leave
    /// tasks in the inject queue with the local one empty.
    fn has_ready_work(&self, handle: &Handle) -> bool {
        !self.tasks.is_empty() || !handle.shared.inject.is_empty()
    }
}

/// A task panicked and the runtime is configured to shut down.
struct Panicked;

impl Context {
    /// Runs up to `event_interval` tasks.
    fn run_batch(&self, mut core: Box<Core>) -> (Box<Core>, Result<(), Panicked>) {
        let handle = &self.handle;
        core.metrics.start_processing_scheduled_tasks();

        let interval = handle.shared.config.event_interval;
        let mut ran = 0;
        while ran < interval && !core.unhandled_panic {
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
        (core, Ok(()))
    }
}

impl CoreGuard<'_> {
    fn drive(self) -> bool {
        let busy = self.enter(|core, context| {
            let handle = &context.handle;
            // The driver's turn: I/O readiness and due timers, ahead of the
            // batch that consumes what they wake.
            let core = context.park_yield(core, handle);
            let (core, ran) = context.run_batch(core);
            if ran.is_err() {
                return (core, Err(Panicked));
            }
            #[cfg(not(windows))]
            let busy = core.has_ready_work(handle);
            // mio's Windows selector issues a socket's readiness poll only
            // at the start of a `poll`: a registration or re-arm from the
            // batch waits for the next driver turn, and until then nothing
            // could signal the host's handle for it. Take that turn now; if
            // it dequeued events of its own, their sockets wait in turn, so
            // the host must drive again before it sleeps.
            #[cfg(windows)]
            let (core, busy) = {
                let core = context.park_yield(core, handle);
                let busy = core.has_ready_work(handle) || handle.driver.io().dequeued();
                (core, busy)
            };
            (core, Ok(busy))
        });

        match busy {
            Ok(busy) => busy,
            Err(Panicked) => panic!(
                "a spawned task panicked and the runtime is configured to shut down on unhandled panic"
            ),
        }
    }
}
