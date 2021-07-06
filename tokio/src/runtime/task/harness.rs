use crate::future::Future;
use crate::runtime::task::core::{Cell, Core, CoreStage, Header, Scheduler, Trailer};
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
    /// Polls the inner future.
    ///
    /// All necessary state checks and transitions are performed.
    ///
    /// Panics raised while polling the future are handled.
    pub(super) fn poll(self) {
        match self.poll_inner() {
            PollFuture::Notified => {
                // Signal yield
                self.core().scheduler.yield_now(Notified(self.to_task()));
                // The ref-count was incremented as part of
                // `transition_to_idle`.
                self.drop_reference();
            }
            PollFuture::DropReference => {
                self.drop_reference();
            }
            PollFuture::Complete(out, is_join_interested) => {
                self.complete(out, is_join_interested);
            }
            PollFuture::None => (),
        }
    }

    fn poll_inner(&self) -> PollFuture<T::Output> {
        let snapshot = match self.scheduler_view().transition_to_running() {
            TransitionToRunning::Ok(snapshot) => snapshot,
            TransitionToRunning::DropReference => return PollFuture::DropReference,
        };

        // The transition to `Running` done above ensures that a lock on the
        // future has been obtained. This also ensures the `*mut T` pointer
        // contains the future (as opposed to the output) and is initialized.

        let waker_ref = waker_ref::<T, S>(self.header());
        let cx = Context::from_waker(&*waker_ref);
        poll_future(self.header(), &self.core().stage, snapshot, cx)
    }

    pub(super) fn dealloc(self) {
        // Release the join waker, if there is one.
        self.trailer().waker.with_mut(drop);

        // Check causality
        self.core().stage.with_mut(drop);
        self.core().scheduler.with_mut(drop);

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
        // Try to unset `JOIN_INTEREST`. This must be done as a first step in
        // case the task concurrently completed.
        if self.header().state.unset_join_interested().is_err() {
            // It is our responsibility to drop the output. This is critical as
            // the task output may not be `Send` and as such must remain with
            // the scheduler or `JoinHandle`. i.e. if the output remains in the
            // task structure until the task is deallocated, it may be dropped
            // by a Waker on any arbitrary thread.
            self.core().stage.drop_future_or_output();
        }

        // Drop the `JoinHandle` reference, possibly deallocating the task
        self.drop_reference();
    }

    // ===== waker behavior =====

    pub(super) fn wake_by_val(self) {
        self.wake_by_ref();
        self.drop_reference();
    }

    pub(super) fn wake_by_ref(&self) {
        if self.header().state.transition_to_notified() {
            self.core().scheduler.schedule(Notified(self.to_task()));
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

    /// Forcibly shutdown the task
    ///
    /// Attempt to transition to `Running` in order to forcibly shutdown the
    /// task. If the task is currently running or in a state of completion, then
    /// there is nothing further to do. When the task completes running, it will
    /// notice the `CANCELLED` bit and finalize the task.
    pub(super) fn shutdown(self) {
        if !self.header().state.transition_to_shutdown() {
            // The task is concurrently running. No further work needed.
            return;
        }

        // By transitioning the lifecycle to `Running`, we have permission to
        // drop the future.
        let err = cancel_task(&self.core().stage);
        self.complete(Err(err), true)
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

    // ====== internal ======

    fn complete(self, output: super::Result<T::Output>, is_join_interested: bool) {
        if is_join_interested {
            // Store the output. The future has already been dropped
            //
            // Safety: Mutual exclusion is obtained by having transitioned the task
            // state -> Running
            let stage = &self.core().stage;
            stage.store_output(output);

            // Transition to `Complete`, notifying the `JoinHandle` if necessary.
            transition_to_complete(self.header(), stage, &self.trailer());
        }

        // The task has completed execution and will no longer be scheduled.
        //
        // Attempts to batch a ref-dec with the state transition below.

        if self
            .scheduler_view()
            .transition_to_terminal(is_join_interested)
        {
            self.dealloc()
        }
    }

    fn to_task(&self) -> Task<S> {
        self.scheduler_view().to_task()
    }
}

enum TransitionToRunning {
    Ok(Snapshot),
    DropReference,
}

struct SchedulerView<'a, S> {
    header: &'a Header,
    scheduler: &'a Scheduler<S>,
}

impl<'a, S> SchedulerView<'a, S>
where
    S: Schedule,
{
    fn to_task(&self) -> Task<S> {
        // SAFETY The header is from the same struct containing the scheduler `S` so  the cast is safe
        unsafe { Task::from_raw(self.header.into()) }
    }

    /// Returns true if the task should be deallocated.
    fn transition_to_terminal(&self, is_join_interested: bool) -> bool {
        let ref_dec = if self.scheduler.is_bound() {
            if let Some(task) = self.scheduler.release(self.to_task()) {
                mem::forget(task);
                true
            } else {
                false
            }
        } else {
            false
        };

        // This might deallocate
        let snapshot = self
            .header
            .state
            .transition_to_terminal(!is_join_interested, ref_dec);

        snapshot.ref_count() == 0
    }

    fn transition_to_running(&self) -> TransitionToRunning {
        // If this is the first time the task is polled, the task will be bound
        // to the scheduler, in which case the task ref count must be
        // incremented.
        let is_not_bound = !self.scheduler.is_bound();

        // Transition the task to the running state.
        //
        // A failure to transition here indicates the task has been cancelled
        // while in the run queue pending execution.
        let snapshot = match self.header.state.transition_to_running(is_not_bound) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                // The task was shutdown while in the run queue. At this point,
                // we just hold a ref counted reference. Since we do not have access to it here
                // return `DropReference` so the caller drops it.
                return TransitionToRunning::DropReference;
            }
        };

        if is_not_bound {
            // Ensure the task is bound to a scheduler instance. Since this is
            // the first time polling the task, a scheduler instance is pulled
            // from the local context and assigned to the task.
            //
            // The scheduler maintains ownership of the task and responds to
            // `wake` calls.
            //
            // The task reference count has been incremented.
            //
            // Safety: Since we have unique access to the task so that we can
            // safely call `bind_scheduler`.
            self.scheduler.bind_scheduler(self.to_task());
        }
        TransitionToRunning::Ok(snapshot)
    }
}

/// Transitions the task's lifecycle to `Complete`. Notifies the
/// `JoinHandle` if it still has interest in the completion.
fn transition_to_complete<T>(header: &Header, stage: &CoreStage<T>, trailer: &Trailer)
where
    T: Future,
{
    // Transition the task's lifecycle to `Complete` and get a snapshot of
    // the task's sate.
    let snapshot = header.state.transition_to_complete();

    if !snapshot.is_join_interested() {
        // The `JoinHandle` is not interested in the output of this task. It
        // is our responsibility to drop the output.
        stage.drop_future_or_output();
    } else if snapshot.has_join_waker() {
        // Notify the join handle. The previous transition obtains the
        // lock on the waker cell.
        trailer.wake_join();
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

enum PollFuture<T> {
    Complete(Result<T, JoinError>, bool),
    DropReference,
    Notified,
    None,
}

fn cancel_task<T: Future>(stage: &CoreStage<T>) -> JoinError {
    // Drop the future from a panic guard.
    let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        stage.drop_future_or_output();
    }));

    if let Err(err) = res {
        // Dropping the future panicked, complete the join
        // handle with the panic to avoid dropping the panic
        // on the ground.
        JoinError::panic(err)
    } else {
        JoinError::cancelled()
    }
}

fn poll_future<T: Future>(
    header: &Header,
    core: &CoreStage<T>,
    snapshot: Snapshot,
    cx: Context<'_>,
) -> PollFuture<T::Output> {
    if snapshot.is_cancelled() {
        PollFuture::Complete(Err(JoinError::cancelled()), snapshot.is_join_interested())
    } else {
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            struct Guard<'a, T: Future> {
                core: &'a CoreStage<T>,
            }

            impl<T: Future> Drop for Guard<'_, T> {
                fn drop(&mut self) {
                    self.core.drop_future_or_output();
                }
            }

            let guard = Guard { core };

            let res = guard.core.poll(cx);

            // prevent the guard from dropping the future
            mem::forget(guard);

            res
        }));
        match res {
            Ok(Poll::Pending) => match header.state.transition_to_idle() {
                Ok(snapshot) => {
                    if snapshot.is_notified() {
                        PollFuture::Notified
                    } else {
                        PollFuture::None
                    }
                }
                Err(_) => PollFuture::Complete(Err(cancel_task(core)), true),
            },
            Ok(Poll::Ready(ok)) => PollFuture::Complete(Ok(ok), snapshot.is_join_interested()),
            Err(err) => {
                PollFuture::Complete(Err(JoinError::panic(err)), snapshot.is_join_interested())
            }
        }
    }
}
