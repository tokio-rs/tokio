//! The event-loop drive: one `current_thread` scheduler batch from host
//! context, never waiting. `block_on` does not come through here; it runs the
//! native loop and suspends at the park leaf. See `runtime::event_loop`.

use super::{CoreGuard, CurrentThread, Handle};
use crate::loom::sync::Arc;

use std::thread;

impl CurrentThread {
    /// Run up to `event_interval` tasks, then a non-blocking driver turn
    /// (due timers, I/O readiness, deferred wakers). Returns whether ready
    /// work remains. Must be called inside `enter_runtime` on an empty stack.
    pub(crate) fn drive_batch(&self, handle: &Arc<Handle>) -> bool {
        // The core is checked out by a drive or `block_on` further up this
        // stack, which observes any work this wake announced.
        let Some(core) = self.take_core(handle) else {
            return false;
        };
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core.drive_batch()
    }
}

impl CoreGuard<'_> {
    fn drive_batch(self) -> bool {
        let busy = self.enter(|mut core, context| {
            let handle = &context.handle;

            core.metrics.start_processing_scheduled_tasks();

            for _ in 0..handle.shared.config.event_interval {
                if core.unhandled_panic {
                    core.metrics.end_processing_scheduled_tasks();
                    return (core, None);
                }

                core.tick();

                let Some(task) = core.next_task(handle) else {
                    break;
                };
                let task = handle.shared.owned.assert_owner(task);
                core = context.run_task(task, core);
            }

            core.metrics.end_processing_scheduled_tasks();

            // The driver turn a park would do, without the wait.
            core = context.park_yield(core, handle);

            let busy = !core.tasks.is_empty() || handle.shared.inject.len() > 0;
            (core, Some(busy))
        });

        match busy {
            Some(busy) => busy,
            None => panic!(
                "a spawned task panicked and the runtime is configured to shut down on unhandled panic"
            ),
        }
    }
}
