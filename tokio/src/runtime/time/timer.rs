use super::wheel::EntryHandle;
use crate::runtime::scheduler::Handle as SchedulerHandle;
use crate::runtime::time::wheel::cancellation_queue::Sender;
use crate::runtime::time::wheel::Insert;
use crate::runtime::time::Wheel;
use crate::time::Instant;
use crate::util::error::RUNTIME_SHUTTING_DOWN_ERROR;

use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) struct Timer {
    sched_handle: SchedulerHandle,

    /// The entry in the timing wheel.
    ///
    /// This is `None` if the timer has been deregistered.
    entry: Option<EntryHandle>,

    /// The deadline for the timer.
    deadline: Instant,
}

impl std::fmt::Debug for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timer")
            .field("deadline", &self.deadline)
            .finish()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(entry) = self.entry.take() {
            entry.transition_to_cancelling();
        }
    }
}

impl Timer {
    #[track_caller]
    pub(crate) fn new(sched_hdl: SchedulerHandle, deadline: Instant) -> Self {
        // Panic if the time driver is not enabled
        let _ = sched_hdl.driver().time();
        Timer {
            sched_handle: sched_hdl,
            entry: None,
            deadline,
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        self.entry.as_ref().is_some_and(|entry| entry.is_woken_up())
    }

    fn register(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();

        with_current_wheel(&this.sched_handle, |maybe_wheel| {
            let deadline = deadline_to_tick(&this.sched_handle, this.deadline);
            let hdl = EntryHandle::new(deadline, cx.waker());
            if let Some((wheel, tx, is_shutdown)) = maybe_wheel {
                assert!(!is_shutdown, "{RUNTIME_SHUTTING_DOWN_ERROR}");
                // Safety: the entry is not registered yet
                match unsafe { wheel.insert(hdl.clone(), tx) } {
                    Insert::Success => {
                        this.entry = Some(hdl);
                        Poll::Pending
                    }
                    Insert::Elapsed => Poll::Ready(()),
                    Insert::Cancelling => Poll::Pending,
                }
            } else {
                this.entry = Some(hdl.clone());
                push_from_remote(&this.sched_handle, hdl);
                Poll::Pending
            }
        })
    }

    pub(crate) fn poll_elapsed(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match self.entry.as_ref() {
            Some(entry) if entry.is_woken_up() => Poll::Ready(()),
            Some(entry) => {
                entry.register_waker(cx.waker());
                Poll::Pending
            }
            None => self.register(cx),
        }
    }
}

pub(super) fn with_current_wheel<F, R>(hdl: &SchedulerHandle, f: F) -> R
where
    F: FnOnce(Option<(&mut Wheel, Sender, bool)>) -> R,
{
    #[cfg(not(feature = "rt"))]
    {
        let (_, _) = (hdl, f);
        panic!("Tokio runtime is not enabled, cannot access the current wheel");
    }

    #[cfg(feature = "rt")]
    {
        use crate::loom::sync::Arc;
        use crate::runtime::context;
        use crate::runtime::scheduler::Context;
        use crate::runtime::scheduler::Handle::CurrentThread;
        #[cfg(feature = "rt-multi-thread")]
        use crate::runtime::scheduler::Handle::MultiThread;

        let is_same_rt = context::with_current(|cur_hdl| match (cur_hdl, hdl) {
            (CurrentThread(cur_hdl), CurrentThread(hdl)) => Arc::ptr_eq(cur_hdl, hdl),
            #[cfg(feature = "rt-multi-thread")]
            (MultiThread(cur_hdl), MultiThread(hdl)) => Arc::ptr_eq(cur_hdl, hdl),
            #[cfg(feature = "rt-multi-thread")]
            // this above cfg is needed to avoid the compiler warning reported by:
            // cargo check -Zbuild-std --target target-specs/i686-unknown-linux-gnu.json \
            //     --manifest-path tokio/Cargo.toml --no-default-features \
            //     --features test-util`
            // error: unreachable pattern
            //    --> tokio/src/runtime/time/timer.rs:118:13
            //     |
            // 115 |             (CurrentThread(cur_hdl), CurrentThread(hdl)) => Arc::ptr_eq(cur_hdl, hdl),
            //     |             -------------------------------------------- matches all the relevant values
            // ...
            // 118 |             _ => false,
            //     |             ^ no value can reach this
            _ => false,
        })
        .unwrap_or_default();

        if !is_same_rt {
            // We don't want to create the timer in one runtime,
            // but register it in a different runtime's timer wheel.
            f(None)
        } else {
            context::with_scheduler(|maybe_cx| match maybe_cx {
                Some(Context::CurrentThread(cx)) => cx.with_wheel(f),
                #[cfg(feature = "rt-multi-thread")]
                Some(Context::MultiThread(cx)) => cx.with_wheel(f),
                None => f(None),
            })
        }
    }
}

fn push_from_remote(sched_hdl: &SchedulerHandle, entry_hdl: EntryHandle) {
    #[cfg(not(feature = "rt"))]
    {
        let (_, _) = (sched_hdl, entry_hdl);
        panic!("Tokio runtime is not enabled, cannot access the current wheel");
    }

    #[cfg(feature = "rt")]
    {
        use crate::runtime::scheduler::Handle::CurrentThread;
        #[cfg(feature = "rt-multi-thread")]
        use crate::runtime::scheduler::Handle::MultiThread;

        match sched_hdl {
            CurrentThread(hdl) => {
                assert!(!hdl.is_shutdown(), "{RUNTIME_SHUTTING_DOWN_ERROR}");
                hdl.push_remote_timer(entry_hdl)
            }
            #[cfg(feature = "rt-multi-thread")]
            MultiThread(hdl) => {
                assert!(!hdl.is_shutdown(), "{RUNTIME_SHUTTING_DOWN_ERROR}");
                hdl.push_remote_timer(entry_hdl)
            }
        }
    }
}

fn deadline_to_tick(sched_hdl: &SchedulerHandle, deadline: Instant) -> u64 {
    let time_hdl = sched_hdl.driver().time();
    time_hdl.time_source().deadline_to_tick(deadline)
}
