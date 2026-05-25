use super::{EntryHandle, TempLocalContext};
use crate::runtime::scheduler;
use crate::time::Instant;

use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
use crate::util::error::RUNTIME_SHUTTING_DOWN_ERROR;

pub(crate) struct Timer {
    /// The entry in the timing wheel.
    entry: EntryHandle,
}

impl std::fmt::Debug for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timer").finish()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.entry.cancel();
    }
}

impl Timer {
    #[track_caller]
    pub(crate) fn new(
        handle: scheduler::Handle,
        deadline: Instant,
    ) -> Result<Self, scheduler::Handle> {
        let tick = deadline_to_tick(&handle, deadline);
        with_current_temp_local_context(|ctx| match ctx {
            Some(TempLocalContext::Running { wheel, canc_tx: _ }) if tick <= wheel.elapsed() => {
                Err(handle)
            }
            Some(TempLocalContext::Running { wheel, canc_tx }) => {
                let entry = EntryHandle::new(tick);
                unsafe { wheel.insert(entry.clone(), canc_tx.clone()) }
                Ok(Timer { entry })
            }
            Some(TempLocalContext::Shutdown) => panic!("{RUNTIME_SHUTTING_DOWN_ERROR}"),
            None => {
                let entry = EntryHandle::new(tick);
                push_from_remote(&handle, entry.clone());
                Ok(Timer { entry })
            }
        })
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        self.entry.is_woken_up()
    }

    pub(crate) fn poll_elapsed(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.entry.poll(cx)
    }
}

fn with_current_temp_local_context<F, R>(f: F) -> R
where
    F: FnOnce(Option<TempLocalContext<'_>>) -> R,
{
    #[cfg(not(feature = "rt"))]
    {
        let _ = f;
        panic!("Tokio runtime is not enabled, cannot access the current wheel");
    }

    #[cfg(feature = "rt")]
    {
        use crate::runtime::context;

        context::with_scheduler(|maybe_cx| match maybe_cx {
            Some(cx) => cx.expect_multi_thread().with_time_temp_local_context(f),
            None => f(None),
        })
    }
}

fn push_from_remote(sched_hdl: &scheduler::Handle, entry_hdl: EntryHandle) {
    #[cfg(not(feature = "rt"))]
    {
        let (_, _) = (sched_hdl, entry_hdl);
        panic!("Tokio runtime is not enabled, cannot access the current wheel");
    }

    #[cfg(feature = "rt")]
    {
        assert!(!sched_hdl.is_shutdown(), "{RUNTIME_SHUTTING_DOWN_ERROR}");
        sched_hdl.push_remote_timer(entry_hdl);
    }
}

fn deadline_to_tick(sched_hdl: &scheduler::Handle, deadline: Instant) -> u64 {
    let time_hdl = sched_hdl.driver().time();
    time_hdl.time_source().deadline_to_tick(deadline)
}
