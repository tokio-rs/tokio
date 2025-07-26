use super::wheel::EntryHandle;
use crate::{runtime::time::Wheel, time::Instant, util::error::RUNTIME_SHUTTING_DOWN_ERROR};
use std::{
    pin::Pin,
    sync::mpsc,
    task::{Context, Poll},
};

pub(crate) struct Timer {
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
        Pin::new(self).cancel();
    }
}

impl Timer {
    pub(crate) fn new(deadline: Instant) -> Self {
        // dbg!("Creating timer with deadline: {:?}", deadline);
        Timer {
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

        with_current_wheel(|maybe_wheel| {
            let deadline = deadline_to_tick(this.deadline);
            let hdl = EntryHandle::new(deadline, cx.waker());
            if let Some((wheel, tx)) = maybe_wheel {
                if unsafe { wheel.insert(hdl.clone(), tx) } {
                    this.entry = Some(hdl);
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            } else {
                this.entry = Some(hdl.clone());
                push_from_remote(hdl);
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

    pub(crate) fn cancel(self: Pin<&mut Self>) {
        if let Some(entry) = self.get_mut().entry.take() {
            entry.cancel();
        }
    }
}

fn with_current_wheel<F, R>(f: F) -> R
where
    F: FnOnce(Option<(&mut Wheel, mpsc::Sender<EntryHandle>)>) -> R,
{
    #[cfg(feature = "rt")]
    use crate::runtime::scheduler::Context::CurrentThread;
    #[cfg(feature = "rt-multi-thread")]
    use crate::runtime::scheduler::Context::MultiThread;

    #[cfg(not(feature = "rt"))]
    let _ = f;

    #[cfg(not(feature = "rt"))]
    panic!("Tokio runtime is not enabled, cannot access the current wheel");

    #[cfg(feature = "rt")]
    crate::runtime::context::with_scheduler(|maybe_cx| match maybe_cx {
        Some(CurrentThread(cx)) => cx.with_wheel(f),
        #[cfg(feature = "rt-multi-thread")]
        Some(MultiThread(cx)) => cx.with_wheel(f),
        None => f(None),
    })
}

fn push_from_remote(hdl: EntryHandle) {
    #[cfg(feature = "rt")]
    use crate::runtime::scheduler::Handle::CurrentThread;
    #[cfg(feature = "rt-multi-thread")]
    use crate::runtime::scheduler::Handle::MultiThread;

    #[cfg(not(feature = "rt"))]
    let _ = hdl;
    #[cfg(not(feature = "rt"))]
    panic!("Tokio runtime is not enabled, cannot access the current wheel");

    #[cfg(feature = "rt")]
    crate::runtime::context::with_current(|sched_hdl| match sched_hdl {
        CurrentThread(sched_hdl) => sched_hdl.push_remote_timer(hdl),
        #[cfg(feature = "rt-multi-thread")]
        MultiThread(sched_hdl) => sched_hdl.push_remote_timer(hdl),
    })
    .unwrap();
}

fn deadline_to_tick(deadline: Instant) -> u64 {
    let binding = crate::runtime::scheduler::Handle::current();
    let hdl = binding.driver().time();

    if hdl.is_shutdown() {
        panic!("{RUNTIME_SHUTTING_DOWN_ERROR}");
    }

    hdl.time_source().deadline_to_tick(deadline)
}
