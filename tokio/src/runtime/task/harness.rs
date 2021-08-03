use crate::future::Future;
use crate::runtime::task::core::{Cell, Core, CoreStage, Header, Trailer};
use crate::runtime::task::state::Snapshot;
use crate::runtime::task::waker::waker_ref;
use crate::runtime::task::{JoinError, Notified, Schedule, Task};

use std::mem;
use std::panic;
use std::ptr::NonNull;
use std::mem::ManuallyDrop;
use std::task::{Context, Poll, Waker};

/// Typed raw task handle
pub(super) struct Harness<T: Future, S: 'static> {
    cell: NonNull<Cell<T, S>>,
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: 'static,
{
    pub(super) unsafe fn from_raw(ptr: NonNull<Header>) -> Harness<T, S> {
        Harness {
            cell: ptr.cast::<Cell<T, S>>(),
        }
    }

    fn header(&self) -> &Header {
        unsafe { &self.cell.as_ref().header }
    }

    fn trailer(&self) -> &Trailer {
        unsafe { &self.cell.as_ref().trailer }
    }

    fn core(&self) -> &Core<T, S> {
        unsafe { &self.cell.as_ref().core }
    }
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: Schedule,
{
    /// Polls the inner future.
    ///
    /// All necessary state checks and transitions are performed.
    ///
    /// Panics raised while polling the future are handled.
    pub(super) fn poll(self) {
        println!("start poll");
        match self.poll_inner() {
            PollFuture::Notified => {
                // Signal yield
                println!("yield");
                self.core().scheduler.yield_now(Notified(self.to_task()));
            }
            PollFuture::Complete => {
                self.complete();
            }
            PollFuture::Dealloc => {
                self.dealloc();
            },
            PollFuture::Done => (),
        }
        println!("stop poll");
    }

    /// Poll the task and cancel it if necessary. This consumes the ref-count
    /// unless the return value is Complete.
    fn poll_inner(&self) -> PollFuture {
        use super::state::{TransitionToIdle, TransitionToRunning};

        match self.header().state.transition_to_running() {
            TransitionToRunning::Success => {
                let waker_ref = waker_ref::<T, S>(self.header());
                let cx = Context::from_waker(&*waker_ref);
                let res = poll_future(&self.core().stage, cx);

                if res == Poll::Ready(()) {
                    // The future completed. Move on to complete the task.
                    return PollFuture::Complete;
                }

                match self.header().state.transition_to_idle() {
                    TransitionToIdle::Ok => PollFuture::Done,
                    TransitionToIdle::OkNotified => PollFuture::Notified,
                    TransitionToIdle::OkDealloc => PollFuture::Dealloc,
                    TransitionToIdle::Cancelled => {
                        // The transition to idle failed because the task was
                        // cancelled during the poll.

                        println!("cancelled after");
                        cancel_task(&self.core().stage);
                        PollFuture::Complete
                    },
                }
            },
            TransitionToRunning::Cancelled => {
                println!("cancelled before");
                cancel_task(&self.core().stage);
                PollFuture::Complete
            },
            TransitionToRunning::Failed => PollFuture::Done,
            TransitionToRunning::Dealloc => PollFuture::Dealloc,
        }
    }

    /// Forcibly shutdown the task
    ///
    /// Attempt to transition to `Running` in order to forcibly shutdown the
    /// task. If the task is currently running or in a state of completion, then
    /// there is nothing further to do. When the task completes running, it will
    /// notice the `CANCELLED` bit and finalize the task.
    pub(super) fn shutdown(self) {
        if !self.header().state.transition_to_shutdown() {
            // The task is concurrently running. No further work needed.
            self.drop_reference();
            return;
        }

        // By transitioning the lifecycle to `Running`, we have permission to
        // drop the future.
        cancel_task(&self.core().stage);
        self.complete();
    }

    pub(super) fn dealloc(self) {
        // Release the join waker, if there is one.
        self.trailer().waker.with_mut(drop);

        // Check causality
        self.core().stage.with_mut(drop);

        unsafe {
            drop(Box::from_raw(self.cell.as_ptr()));
        }
    }

    // ===== join handle =====

    /// Read the task output into `dst`.
    pub(super) fn try_read_output(self, dst: &mut Poll<super::Result<T::Output>>, waker: &Waker) {
        if can_read_output(self.header(), self.trailer(), waker) {
            *dst = Poll::Ready(self.core().stage.take_output());
        }
    }

    pub(super) fn drop_join_handle_slow(self) {
        let mut maybe_panic = None;

        // Try to unset `JOIN_INTEREST`. This must be done as a first step in
        // case the task concurrently completed.
        if self.header().state.unset_join_interested().is_err() {
            // It is our responsibility to drop the output. This is critical as
            // the task output may not be `Send` and as such must remain with
            // the scheduler or `JoinHandle`. i.e. if the output remains in the
            // task structure until the task is deallocated, it may be dropped
            // by a Waker on any arbitrary thread.
            let panic = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                self.core().stage.drop_future_or_output();
            }));

            if let Err(panic) = panic {
                maybe_panic = Some(panic);
            }
        }

        // Drop the `JoinHandle` reference, possibly deallocating the task
        self.drop_reference();

        if let Some(panic) = maybe_panic {
            panic::resume_unwind(panic);
        }
    }

    /// Remotely abort the task
    ///
    /// This is similar to `shutdown` except that it asks the runtime to perform
    /// the shutdown. This is necessary to avoid the shutdown happening in the
    /// wrong thread for non-Send tasks.
    pub(super) fn remote_abort(self) {
        if self.header().state.transition_to_notified_and_cancel() {
            self.core().scheduler.schedule(Notified(self.to_task()));
        }
    }

    // ===== waker behavior =====

    pub(super) fn wake_by_val(self) {
        use super::state::TransitionToNotifiedByVal;

        println!("wake by val");
        match self.header().state.transition_to_notified_by_val() {
            TransitionToNotifiedByVal::Submit => {
                self.core().scheduler.schedule(Notified(self.to_task()));
            },
            TransitionToNotifiedByVal::Dealloc => {
                self.dealloc();
            },
            TransitionToNotifiedByVal::DoNothing => {},
        }
    }

    pub(super) fn wake_by_ref(&self) {
        use super::state::TransitionToNotifiedByRef;

        println!("wake by ref");
        match self.header().state.transition_to_notified_by_ref() {
            TransitionToNotifiedByRef::Submit => {
                self.core().scheduler.schedule(Notified(self.to_task()));
            },
            TransitionToNotifiedByRef::DoNothing => {},
        }
    }

    pub(super) fn drop_reference(self) {
        if self.header().state.ref_dec() {
            self.dealloc();
        }
    }

    #[cfg(all(tokio_unstable, feature = "tracing"))]
    pub(super) fn id(&self) -> Option<&tracing::Id> {
        self.header().id.as_ref()
    }

    // ====== internal ======

    /// Complete the task. This method assumes that the state is RUNNING.
    fn complete(self) {
        // The future has completed and its output has been written to the task
        // stage. We transition from running to complete.

        let snapshot = self.header().state.transition_to_complete();

        // We catch panics here in case dropping the future or waking the
        // JoinHandle panics.
        let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            if !snapshot.is_join_interested() {
                // The `JoinHandle` is not interested in the output of
                // this task. It is our responsibility to drop the
                // output.
                self.core().stage.drop_future_or_output();
            } else if snapshot.has_join_waker() {
                // Notify the join handle. The previous transition obtains the
                // lock on the waker cell.
                self.trailer().wake_join();
            }
        }));

        // The task has completed execution and will no longer be scheduled.
        let num_release = self.release();

        if self.header().state.transition_to_terminal(num_release) {
            self.dealloc();
        }
    }

    /// Release the task from the scheduler. Returns the number of ref-counts
    /// that should be decremented.
    fn release(&self) -> usize {
        let me = ManuallyDrop::new(self.to_task());

        if let Some(task) = self.core().scheduler.release(&me) {
            mem::forget(task);
            2
        } else {
            1
        }
    }

    fn to_task(&self) -> Task<S> {
        // safety: The header is at the beginning of the cell, so this cast is
        // safe.
        unsafe { Task::from_raw(self.cell.cast()) }
    }
}

fn can_read_output(header: &Header, trailer: &Trailer, waker: &Waker) -> bool {
    // Load a snapshot of the current task state
    let snapshot = header.state.load();

    debug_assert!(snapshot.is_join_interested());

    if !snapshot.is_complete() {
        // The waker must be stored in the task struct.
        let res = if snapshot.has_join_waker() {
            // There already is a waker stored in the struct. If it matches
            // the provided waker, then there is no further work to do.
            // Otherwise, the waker must be swapped.
            let will_wake = unsafe {
                // Safety: when `JOIN_INTEREST` is set, only `JOIN_HANDLE`
                // may mutate the `waker` field.
                trailer.will_wake(waker)
            };

            if will_wake {
                // The task is not complete **and** the waker is up to date,
                // there is nothing further that needs to be done.
                return false;
            }

            // Unset the `JOIN_WAKER` to gain mutable access to the `waker`
            // field then update the field with the new join worker.
            //
            // This requires two atomic operations, unsetting the bit and
            // then resetting it. If the task transitions to complete
            // concurrently to either one of those operations, then setting
            // the join waker fails and we proceed to reading the task
            // output.
            header
                .state
                .unset_waker()
                .and_then(|snapshot| set_join_waker(header, trailer, waker.clone(), snapshot))
        } else {
            set_join_waker(header, trailer, waker.clone(), snapshot)
        };

        match res {
            Ok(_) => return false,
            Err(snapshot) => {
                assert!(snapshot.is_complete());
            }
        }
    }
    true
}

fn set_join_waker(
    header: &Header,
    trailer: &Trailer,
    waker: Waker,
    snapshot: Snapshot,
) -> Result<Snapshot, Snapshot> {
    assert!(snapshot.is_join_interested());
    assert!(!snapshot.has_join_waker());

    // Safety: Only the `JoinHandle` may set the `waker` field. When
    // `JOIN_INTEREST` is **not** set, nothing else will touch the field.
    unsafe {
        trailer.set_waker(Some(waker));
    }

    // Update the `JoinWaker` state accordingly
    let res = header.state.set_join_waker();

    // If the state could not be updated, then clear the join waker
    if res.is_err() {
        unsafe {
            trailer.set_waker(None);
        }
    }

    res
}

enum PollFuture {
    Complete,
    Notified,
    Done,
    Dealloc,
}

/// Cancel the task and store the appropriate error in the stage field.
fn cancel_task<T: Future>(stage: &CoreStage<T>) {
    // Drop the future from a panic guard.
    let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        stage.drop_future_or_output();
    }));

    match res {
        Ok(()) => {
            stage.store_output(Err(JoinError::cancelled()));
        }
        Err(panic) => {
            stage.store_output(Err(JoinError::panic(panic)));
        }
    }
}

/// Poll the future. If the future completes, the output is written to the
/// stage field.
fn poll_future<T: Future>(core: &CoreStage<T>, cx: Context<'_>) -> Poll<()> {
    // Poll the future.
    let output = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        struct Guard<'a, T: Future> {
            core: &'a CoreStage<T>,
        }
        impl<'a, T: Future> Drop for Guard<'a, T> {
            fn drop(&mut self) {
                // If the future panics on poll, we drop it inside the panic
                // guard.
                self.core.drop_future_or_output();
            }
        }
        let guard = Guard { core };
        let res = guard.core.poll(cx);
        mem::forget(guard);
        res
    }));

    // Prepare output for being placed in the core stage.
    let output = match output {
        Ok(Poll::Pending) => return Poll::Pending,
        Ok(Poll::Ready(output)) => Ok(output),
        Err(panic) => Err(JoinError::panic(panic)),
    };

    // Catch and ignore panics if the future panics on drop.
    let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        core.store_output(output);
    }));

    Poll::Ready(())
}
