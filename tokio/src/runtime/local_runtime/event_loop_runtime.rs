use crate::runtime::event_loop::EventLoopState;
use crate::runtime::{Handle, LocalRuntime};
use crate::task::JoinError;

use std::future::Future;
use std::sync::Arc;

/// A [`LocalRuntime`] driven by the host JavaScript event loop on
/// `wasm32-unknown-emscripten`, instead of by parking a thread.
///
/// Built with [`Builder::build_event_loop_runtime`]. Submit roots with
/// [`schedule`](Self::schedule): the runtime runs them in batches from host
/// callbacks, arming a host timer for its soonest timer deadline and driving
/// on socket readiness, so scheduled work is self-sustaining once control
/// returns to the host. Any number of event-loop runtimes may coexist on the
/// thread.
///
/// `block_on` is rejected on an event-loop runtime, like a nested runtime:
/// its wait is the host loop, so no stack can hold the result. A drive from a
/// host callback enters the runtime, so it panics as a nested runtime if it
/// lands while another runtime's `block_on` is suspended under JSPI.
///
/// Dropping the `EventLoopRuntime` drops the runtime with native semantics:
/// in-flight roots are dropped, and armed host callbacks resolve to nothing.
///
/// [`Builder::build_event_loop_runtime`]: crate::runtime::Builder::build_event_loop_runtime
#[derive(Debug)]
pub struct EventLoopRuntime {
    state: Arc<EventLoopState>,
}

impl EventLoopRuntime {
    pub(crate) fn new(state: Arc<EventLoopState>) -> EventLoopRuntime {
        EventLoopRuntime { state }
    }

    /// Queues `future` as a root on this runtime, delivering its outcome to
    /// `on_complete` once it resolves. The root is queued and an immediate
    /// drive is armed; it never runs before `schedule` returns (call
    /// [`drive`](Self::drive) to run it before returning to the host).
    ///
    /// The future need not be `Send`. A panic in it is caught and delivered
    /// as `Err(JoinError)` rather than unwinding into the host callback, so a
    /// `Promise`-returning bridge can map `Ok`/`Err` to resolve/reject.
    pub fn schedule<F, C>(&self, future: F, on_complete: C)
    where
        F: Future + 'static,
        F::Output: 'static,
        C: FnOnce(Result<F::Output, JoinError>) + 'static,
    {
        let join = self.state.runtime().spawn_local(future);
        drop(self.state.runtime().spawn_local(async move {
            on_complete(join.await);
        }));
        self.state.arm_drive();
    }

    /// Runs one scheduler batch now: ready tasks, due timers and I/O
    /// readiness, then arms the next host wake. Optional; scheduled work is
    /// driven by the host loop either way.
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime, as `block_on` does.
    pub fn drive(&self) {
        self.state.drive();
    }

    /// Returns a handle to this runtime.
    pub fn handle(&self) -> &Handle {
        self.state.runtime().handle()
    }

    /// The underlying [`LocalRuntime`]. Its `block_on` panics on an
    /// event-loop runtime; use [`schedule`](Self::schedule).
    pub fn local(&self) -> &LocalRuntime {
        self.state.runtime()
    }
}
