use crate::future::Future;
use crate::runtime::task::core::{Cell, Core, CoreStage, Header, Trailer};
use crate::runtime::task::state::Snapshot;
use crate::runtime::task::waker::waker_ref;
use crate::runtime::task::{JoinError, Notified, Schedule, Task};

use std::mem;
use std::panic;
use std::ptr::NonNull;
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

    fn as_raw(&self) -> NonNull<Header> {
        self.cell.cast::<Header>()
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

    fn scheduler_view(&self) -> SchedulerView<'_, S> {
        SchedulerView {
            header: self.header(),
            scheduler: &self.core().scheduler,
        }
    }
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: Schedule,
{
    /// Polls the inner future, consuming a Notified.
    pub(super) fn poll(self) {
        let mut operation = PollOperation {
            state: PollOperationState::IdlePoll,
            ownership_notified: true,
            ownership_refcount: 0,
        };

        while operation.is_not_done() {
            self.step_poll_operation(&mut operation);
        }
    }

    /// Forcibly shutdown the task. This consumes a ref-count.
    pub(super) fn shutdown(self) {
        let mut operation = PollOperation {
            state: PollOperationState::IdleCancel,
            ownership_notified: false,
            ownership_refcount: 1,
        };

        while operation.is_not_done() {
            self.step_poll_operation(&mut operation);
        }
    }

    /// Execute the given PollOperation until it reaches the Done state.
    /// This method assumes that the necessary state transitions to reach the
    /// initial state has already been performed.
    fn step_poll_operation(&self, op: &mut PollOperation) {
        match op.state {
            PollOperationState::IdlePoll => {
                // Try to transition to running, failing if someone else has the
                // lock or if the task is completed.
                match self.header().state.transition_to_running() {
                    Ok(snapshot) => {
                        if snapshot.is_cancelled() {
                            op.state = PollOperationState::ShouldCancel;
                        } else {
                            op.state = PollOperationState::ShouldPoll;
                        }
                    }
                    Err(_) => {
                        // This might unset NOTIFIED_DURING_POLL, but there are
                        // only two ways that poll operations can be started:
                        //
                        //  * Calling poll on the unique Notified
                        //  * Cancel the task
                        //
                        // However the transition failing means that the task is
                        // either being cancelled or already completed, so there
                        // is no risk of losing notifications.
                        op.state = PollOperationState::ReleaseRefcount;
                    }
                }
            }
            PollOperationState::IdleCancel => {
                if self.header().state.transition_to_shutdown() {
                    op.state = PollOperationState::ShouldCancel;
                } else {
                    // We just set the CANCELLED bit if it wasn't already set.
                    // Whoever is polling the task will see it when they finish.
                    op.state = PollOperationState::ReleaseRefcount;
                }
            }
            PollOperationState::ShouldPoll => {
                // The ownership is only ref-count if we are cancelled from
                // runtime shutdown. Runtime shutdown should not enter this
                // state.
                debug_assert!(op.ownership_notified);

                self.poll_inner(op);

                debug_assert_ne!(op.state, PollOperationState::ShouldPoll);
            }
            PollOperationState::ShouldCancel => {
                cancel_task(&self.core().stage);
                let state = self.header().state.transition_to_complete();
                op.state = PollOperationState::ShouldComplete(state);
            }
            PollOperationState::ShouldComplete(snapshot) => {
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

                if self.scheduler_view().release() {
                    op.ownership_refcount += 1;
                }
                op.state = PollOperationState::ReleaseRefcount;
            }
            PollOperationState::ReleaseRefcount => {
                let dealloc = if op.ownership_notified {
                    self.header().state.unset_notified(op.ownership_refcount)
                } else if op.ownership_refcount > 0 {
                    self.header().state.ref_dec_several(op.ownership_refcount)
                } else {
                    false
                };

                if dealloc {
                    self.dealloc();
                }

                op.state = PollOperationState::Done;
            }
            PollOperationState::Done => {}
        }
    }

    fn poll_inner(&self, op: &mut PollOperation) {
        let waker_ref = waker_ref::<T, S>(self.header());
        let cx = Context::from_waker(&*waker_ref);
        let output = poll_future(&self.core().stage, cx);
        match output {
            Poll::Pending => {
                use super::state::IdleTransition;
                assert!(op.ownership_notified);
                match self.header().state.transition_to_idle() {
                    IdleTransition::Ok => {
                        // The NOTIFIED bit was consumed by the transition to
                        // idle.
                        op.ownership_notified = false;
                        op.state = PollOperationState::ReleaseRefcount;
                    }
                    IdleTransition::OkDealloc => {
                        // The NOTIFIED bit was consumed by the transition to
                        // idle. There was no ref-count holding on to the
                        // task.
                        op.ownership_notified = false;
                        debug_assert_eq!(op.ownership_refcount, 0);
                        self.dealloc();

                        op.state = PollOperationState::Done;
                    }
                    IdleTransition::OkNotified => {
                        // The NOTIFIED bit was not unset, but we transfer
                        // ownership of the NOTIFIED to the notification below.
                        op.ownership_notified = false;
                        self.core().scheduler.yield_now(self.to_notified());

                        op.state = PollOperationState::ReleaseRefcount;
                    }
                    IdleTransition::Cancelled => {
                        // The transition failed. We are now ready to cancel
                        // the task. The NOTIFIED bit was not consumed.
                        op.state = PollOperationState::ShouldCancel;
                    }
                }
            }
            Poll::Ready(()) => {
                let state = self.header().state.transition_to_complete();
                // The transition to complete does not touch the ref-count or
                // NOTIFIED bit.
                op.state = PollOperationState::ShouldComplete(state);
            }
        }
    }

    pub(super) fn dealloc(&self) {
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

    // ===== waker behavior =====

    pub(super) fn wake_by_val(self) {
        self.wake_by_ref();
        self.drop_reference();
    }

    pub(super) fn wake_by_ref(&self) {
        if self.header().state.transition_to_notified() {
            self.core().scheduler.schedule(self.to_notified());
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

    /// Remotely abort the task
    ///
    /// This is similar to `shutdown` except that it asks the runtime to perform
    /// the shutdown. This is necessary to avoid the shutdown happening in the
    /// wrong thread for non-Send tasks.
    pub(super) fn remote_abort(self) {
        if self.header().state.transition_to_notified_and_cancel() {
            self.core().scheduler.schedule(self.to_notified());
        }
    }

    // ====== internal ======

    fn to_notified(&self) -> Notified<S> {
        unsafe { Notified::from_raw(self.as_raw()) }
    }
}

struct SchedulerView<'a, S> {
    header: &'a Header,
    scheduler: &'a S,
}

impl<'a, S> SchedulerView<'a, S>
where
    S: Schedule,
{
    fn to_task(&self) -> Task<S> {
        // SAFETY The header is from the same struct containing the scheduler `S` so  the cast is safe
        unsafe { Task::from_raw(self.header.into()) }
    }

    /// Release the task from the scheduler. Returns true if a ref_dec should be
    /// performed.
    fn release(&self) -> bool {
        let me = self.to_task();

        let ref_dec = if let Some(task) = self.scheduler.release(&me) {
            mem::forget(task);
            true
        } else {
            false
        };

        mem::forget(me);

        ref_dec
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum PollOperationState {
    /// We are in an idle state. We should attempt to transition to a running
    /// state, then move to `ShouldPoll` or `ShouldCancel` as appropriate.
    IdlePoll,

    /// We are in an idle state. We should attempt to transition to a running
    /// state, then move to `ShouldCancel`.
    ///
    /// If the transition to running fails, the CANCELLED bit should be set.
    IdleCancel,

    /// We are just about to poll the task. The task is not completed and we
    /// hold the lock on the state.
    ShouldPoll,

    /// We are just about to cancel the task. The task is not completed and we
    /// hold the lock on the state.
    ShouldCancel,

    /// We are just about to notify the JoinHandle and decrement the
    /// ref-count/NOTIFIED that we own. The COMPLETE bit is set before entering
    /// this state.
    ///
    /// The return value of transition_to_complete is included.
    ShouldComplete(Snapshot),

    /// We are done except that we need to release any NOTIFIED bits or
    /// ref-counts as described in the PollOperation fields.
    ReleaseRefcount,

    /// We finished the operation and no longer hold the lock that allows us to
    /// touch the task. The ref-count or NOTIFIED bit has been consumed at this
    /// point.
    Done,
}

struct PollOperation {
    state: PollOperationState,
    /// True if we need to release a NOTIFIED bit when we reach the
    /// ReleaseRefcount state.
    ownership_notified: bool,
    /// The number of ref-counts to release when we reach the ReleaseRefcount
    /// state.
    ownership_refcount: usize,
}

impl PollOperation {
    fn is_not_done(&self) -> bool {
        !matches!(self.state, PollOperationState::Done)
    }
}

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
