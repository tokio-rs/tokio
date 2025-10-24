use super::wheel::{EntryHandle, EntryState};
use crate::runtime::context;
use crate::runtime::scheduler::Handle as SchedulerHandle;
use crate::runtime::time::wheel::Insert;
use crate::runtime::time::Context as TimeContext;
use crate::time::Instant;

use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(any(feature = "rt", feature = "rt-multi-thread"))]
use crate::util::error::RUNTIME_SHUTTING_DOWN_ERROR;

pub(crate) struct Timer {
    sched_handle: SchedulerHandle,

    /// The entry in the timing wheel.
    ///
    /// - `Some` if the timer is registered / pending / woken up / cancelling.
    /// - `None` if the timer is unregistered.
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
            with_current_wheel(&self.sched_handle, |maybe_time_cx| {
                let state = entry.state();

                let thread_id = match state {
                    EntryState::Unregistered => {
                        entry.transition_to_cancelling();
                        return;
                    }
                    EntryState::Registered(thread_id) | EntryState::Pending(thread_id) => thread_id,
                    EntryState::Cancelling(..) => unreachable!(),
                    EntryState::WokenUp => return,
                };

                let Ok(cur_thread_id) = context::thread_id() else {
                    // current thread is shutting down, we cannot determine the thread id,
                    // so we need to fallback to the cancellation queue.
                    entry.transition_to_cancelling();
                    return;
                };

                if let Some(TimeContext::Running { wheel, canc_tx: _ }) = maybe_time_cx {
                    if thread_id == cur_thread_id {
                        // Safety:
                        // 1. entry is either in slots or pending list
                        // 2. entry is registered in this thread
                        unsafe {
                            wheel.remove(entry);
                        }
                    } else {
                        entry.transition_to_cancelling();
                    }
                } else {
                    entry.transition_to_cancelling();
                }
            });
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

        with_current_wheel(&this.sched_handle, |maybe_time_cx| {
            let deadline = deadline_to_tick(&this.sched_handle, this.deadline);
            let hdl = EntryHandle::new(deadline, cx.waker());
            let thread_id = context::thread_id().ok();

            match (maybe_time_cx, thread_id) {
                (Some(TimeContext::Running { wheel, canc_tx }), Some(thread_id)) => {
                    // Safety: the entry is not registered yet
                    match unsafe { wheel.insert(hdl.clone(), canc_tx.clone(), thread_id) } {
                        Insert::Success => {
                            this.entry = Some(hdl);
                            Poll::Pending
                        }
                        Insert::Elapsed => Poll::Ready(()),
                        Insert::Cancelling => Poll::Pending,
                    }
                }
                #[cfg(feature = "rt-multi-thread")]
                (Some(TimeContext::Shutdown), _) => panic!("{RUNTIME_SHUTTING_DOWN_ERROR}"),
                _ => {
                    this.entry = Some(hdl.clone());
                    push_from_remote(&this.sched_handle, hdl);
                    Poll::Pending
                }
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
    F: FnOnce(Option<TimeContext<'_>>) -> R,
{
    #[cfg(not(feature = "rt"))]
    {
        let (_, _) = (hdl, f);
        panic!("Tokio runtime is not enabled, cannot access the current wheel");
    }

    #[cfg(feature = "rt")]
    {
        use crate::runtime::context;

        let is_same_rt =
            context::with_current(|cur_hdl| cur_hdl.is_same_runtime(hdl)).unwrap_or_default();

        if !is_same_rt {
            // We don't want to create the timer in one runtime,
            // but register it in a different runtime's timer wheel.
            f(None)
        } else {
            context::with_scheduler(|maybe_cx| match maybe_cx {
                Some(cx) => cx.with_wheel(f),
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
        assert!(!sched_hdl.is_shutdown(), "{RUNTIME_SHUTTING_DOWN_ERROR}");
        sched_hdl.push_remote_timer(entry_hdl);
    }
}

fn deadline_to_tick(sched_hdl: &SchedulerHandle, deadline: Instant) -> u64 {
    let time_hdl = sched_hdl.driver().time();
    time_hdl.time_source().deadline_to_tick(deadline)
}
