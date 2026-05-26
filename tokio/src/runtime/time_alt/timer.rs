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
    pub(crate) fn new(handle: scheduler::Handle, deadline: Instant) -> Self {
        let tick = deadline_to_tick(&handle, deadline);
        let entry = with_current_temp_local_context(&handle, |ctx| match ctx {
            Some(TempLocalContext::Running { registration_queue }) => {
                let entry = EntryHandle::new(tick);
                unsafe { registration_queue.push_front(entry.clone()) }
                entry
            }
            #[cfg(feature = "rt-multi-thread")]
            Some(TempLocalContext::Shutdown) => panic!("{RUNTIME_SHUTTING_DOWN_ERROR}"),

            _ => {
                let entry = EntryHandle::new(tick);
                push_from_remote(&handle, entry.clone());
                entry
            }
        });

        Timer { entry }
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        self.entry.is_woken_up()
    }

    pub(crate) fn poll_elapsed(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.entry.poll(cx)
    }
}

fn with_current_temp_local_context<F, R>(sched_hdl: &scheduler::Handle, f: F) -> R
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
        use crate::loom::sync::Arc;
        use crate::runtime::context;

        // There is no compile-time guarantee that the timer is
        // always registered in the same runtime as it was created in,
        // so we need to check it at runtime.
        let is_same_rt = context::with_current(|cur_sched_hdl| {
            use crate::runtime::scheduler::Handle;

            match (sched_hdl, cur_sched_hdl) {
                (Handle::CurrentThread(_), _) => {
                    // this case is impossible as `tokio::runtime::Builder::enable_alt_timer`
                    // is not supported in the current-thread runtime, but we'd better handle it
                    // in case the API is misused in the future.
                    unreachable!("alternative timer is not supported in the current-thread runtime")
                }
                (_, Handle::CurrentThread(_)) => false,
                (Handle::MultiThread(sched_hdl), Handle::MultiThread(cur_sched_hdl)) => {
                    Arc::as_ptr(sched_hdl) == Arc::as_ptr(cur_sched_hdl)
                }
            }
        })
        .unwrap_or_default();

        if !is_same_rt {
            // The timer is being registered from a runtime
            // that is different from the runtime that the timer is created in,
            // so we cannot access `TempLocalContext` of the original runtime.
            return f(None);
        }

        // The timer is being registered from the same runtime that the timer is created in,
        // so we can access `TempLocalContext`.
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
