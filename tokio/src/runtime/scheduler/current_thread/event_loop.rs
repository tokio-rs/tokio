//! Running the `current_thread` scheduler from a host event loop: batches
//! that never wait, and a driver that parks off-thread. See
//! `runtime::event_loop`.

use super::{Context, Core, CoreGuard, CurrentThread, Handle};
use crate::loom::sync::Arc;
use crate::runtime::driver::Driver;

use std::future::Future;
use std::task::Poll::Ready;
use std::thread;

impl CurrentThread {
    /// Run up to `event_interval` tasks. Returns whether ready work remains
    /// queued. Must be called inside `enter_runtime`.
    pub(crate) fn drive_batch(&self, handle: &Arc<Handle>) -> bool {
        let core = self.take_core(handle).expect("core checked out");
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core.drive_batch()
    }

    /// Poll `future` to completion from ready work alone: `None` where a
    /// native `block_on` would park, the future dropped. Also returns whether
    /// ready work remains queued. Must be called inside `enter_runtime`.
    pub(crate) fn block_on_ready<F: Future>(
        &self,
        handle: &Arc<Handle>,
        future: F,
    ) -> (Option<F::Output>, bool) {
        // Nested entry and foreign threads are rejected before this point.
        let core = self.take_core(handle).expect("core checked out");
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core.block_on_ready(future)
    }

    /// Moves the driver out of the core, for parking on another thread.
    #[cfg(not(all(target_os = "emscripten", not(target_feature = "atomics"))))]
    pub(crate) fn take_driver(&self, handle: &Arc<Handle>) -> Option<Driver> {
        let core = self.take_core(handle)?;
        let context = core.context.expect_current_thread();
        let driver = context.core.borrow_mut().as_mut()?.driver.take();
        driver
    }

    /// Returns the driver to the core, for shutdown.
    pub(crate) fn restore_driver(&self, handle: &Arc<Handle>, driver: Driver) {
        if let Some(core) = self.take_core(handle) {
            let context = core.context.expect_current_thread();
            if let Some(core) = context.core.borrow_mut().as_mut() {
                core.driver = Some(driver);
            }
        }
    }
}

impl Core {
    /// Ready work in either queue. A batch ending by `Interval` can leave
    /// tasks in the inject queue with the local one empty; the wake that
    /// queued them was consumed by this drive, so the host must be woken
    /// again for them.
    fn has_ready_work(&self, handle: &Handle) -> bool {
        !self.tasks.is_empty() || !handle.shared.inject.is_empty()
    }
}

enum Batch {
    /// `event_interval` tasks ran; more may be queued.
    Interval,
    /// The queues ran dry.
    Exhausted,
    /// A task panicked and the runtime is configured to shut down.
    Panicked,
}

impl Context {
    fn run_batch(&self, mut core: Box<Core>) -> (Box<Core>, Batch) {
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
        let batch = if core.unhandled_panic {
            Batch::Panicked
        } else if ran == interval {
            Batch::Interval
        } else {
            Batch::Exhausted
        };

        core.metrics.end_processing_scheduled_tasks();
        // Deferred wakers (`yield_now`) are released where a park would; the
        // core must be in the context for their schedules to reach it.
        let (core, ()) = self.enter(core, || self.defer.wake());
        (core, batch)
    }
}

impl CoreGuard<'_> {
    fn drive_batch(self) -> bool {
        let busy = self.enter(|core, context| {
            // No thread parks in the driver on this target: its turn (I/O
            // readiness, due timers) runs here, ahead of the batch that
            // consumes what it wakes. A synchronous probe: the host already
            // has the turn, so it must neither yield nor suspend.
            #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
            let core =
                crate::runtime::jspi::host_turn(|| context.park_yield(core, &context.handle));

            let (core, batch) = context.run_batch(core);
            let busy = match batch {
                Batch::Panicked => None,
                _ => Some(core.has_ready_work(&context.handle)),
            };
            (core, busy)
        });

        match busy {
            Some(busy) => busy,
            None => panic!(
                "a spawned task panicked and the runtime is configured to shut down on unhandled panic"
            ),
        }
    }

    fn block_on_ready<F: Future>(self, future: F) -> (Option<F::Output>, bool) {
        let ret = self.enter(|mut core, context| {
            let waker = Handle::waker_ref(&context.handle);
            let mut cx = std::task::Context::from_waker(&waker);

            pin!(future);

            loop {
                let handle = &context.handle;

                if handle.reset_woken() {
                    let (c, res) = context.enter(core, || {
                        crate::task::coop::budget(|| future.as_mut().poll(&mut cx))
                    });
                    core = c;
                    if let Ready(v) = res {
                        let busy = core.has_ready_work(handle);
                        return (core, Some((Some(v), busy)));
                    }
                }

                let (c, batch) = context.run_batch(core);
                core = c;
                match batch {
                    Batch::Panicked => return (core, None),
                    Batch::Interval => {}
                    Batch::Exhausted if context.has_pending_work(&core) => {}
                    Batch::Exhausted => return (core, Some((None, false))),
                }
            }
        });

        match ret {
            Some(ret) => ret,
            None => panic!(
                "a spawned task panicked and the runtime is configured to shut down on unhandled panic"
            ),
        }
    }
}
