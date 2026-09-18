//! Running the `current_thread` scheduler from a host event loop: batches
//! that never wait, and a driver that parks off-thread. See
//! `runtime::event_loop`.

use super::{Context, Core, CoreGuard, CurrentThread, Handle};
use crate::loom::sync::Arc;
use crate::runtime::driver::Driver;
use crate::runtime::event_loop::WouldBlock;

use std::future::Future;
use std::task::Poll::Ready;
use std::thread;

impl CurrentThread {
    /// Run up to `event_interval` tasks. Returns whether ready work remains
    /// in the local queue. Must be called inside `enter_runtime`.
    pub(crate) fn drive_batch(&self, handle: &Arc<Handle>) -> bool {
        // The core is checked out further up this stack, which observes any
        // work this drive was requested for.
        let Some(core) = self.take_core(handle) else {
            return false;
        };
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core.drive_batch()
    }

    /// Poll `future` to completion from ready work alone, failing where a
    /// native `block_on` would park. Must be called inside `enter_runtime`.
    pub(crate) fn block_on_ready<F: Future>(
        &self,
        handle: &Arc<Handle>,
        future: F,
    ) -> Result<F::Output, WouldBlock> {
        let Some(core) = self.take_core(handle) else {
            return Err(WouldBlock(()));
        };
        handle
            .shared
            .worker_metrics
            .set_thread_id(thread::current().id());
        core.block_on_ready(future)
    }

    /// Moves the driver out of the core, for parking on another thread.
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
            let (core, batch) = context.run_batch(core);
            let busy = match batch {
                Batch::Panicked => None,
                _ => Some(!core.tasks.is_empty()),
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

    fn block_on_ready<F: Future>(self, future: F) -> Result<F::Output, WouldBlock> {
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
                        return (core, Some(Ok(v)));
                    }
                }

                let (c, batch) = context.run_batch(core);
                core = c;
                match batch {
                    Batch::Panicked => return (core, None),
                    Batch::Interval => {}
                    Batch::Exhausted if context.has_pending_work(&core) => {}
                    Batch::Exhausted => return (core, Some(Err(WouldBlock(())))),
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
