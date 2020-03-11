use crate::runtime::task::core::{Cell, Core, Header, Trailer};
use crate::runtime::task::state::Snapshot;
use crate::runtime::task::{JoinError, Notified, Schedule, Task};

use std::future::Future;
use std::mem;
use std::panic;
use std::ptr::NonNull;
use std::task::{Poll, Waker};

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
        // If this is the first time the task is polled, the task will be bound
        // to the scheduler, in which case the task ref count must be
        // incremented.
        let ref_inc = !self.core().is_bound();

        // Transition the task to the running state.
        //
        // A failure to transition here indicates the task has been cancelled
        // while in the run queue pending execution.
        let snapshot = match self.header().state.transition_to_running(ref_inc) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                // The task was shutdown while in the run queue. At this point,
                // we just hold a ref counted reference. Drop it here.
                self.drop_reference();
                return;
            }
        };

        // Ensure the task is bound to a scheduler instance. If this is the
        // first time polling the task, a scheduler instance is pulled from the
        // local context and assigned to the task.
        //
        // The scheduler maintains ownership of the task and responds to `wake`
        // calls.
        //
        // The task reference count has been incremented.
        self.core().bind_scheduler(self.to_task());

        // The transition to `Running` done above ensures that a lock on the
        // future has been obtained. This also ensures the `*mut T` pointer
        // contains the future (as opposed to the output) and is initialized.

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            struct Guard<'a, T: Future, S: Schedule> {
                core: &'a Core<T, S>,
                polled: bool,
            }

            impl<T: Future, S: Schedule> Drop for Guard<'_, T, S> {
                fn drop(&mut self) {
                    if !self.polled {
                        self.core.drop_future_or_output();
                    }
                }
            }

            let mut guard = Guard {
                core: self.core(),
                polled: false,
            };

            // If the task is cancelled, avoid polling it, instead signalling it
            // is complete.
            if snapshot.is_cancelled() {
                Poll::Ready(Err(JoinError::cancelled2()))
            } else {
                let res = guard.core.poll(self.header());

                // prevent the guard from dropping the future
                guard.polled = true;

                res.map(Ok)
            }
        }));

        match res {
            Ok(Poll::Ready(out)) => {
                self.complete(out, snapshot.is_join_interested());
            }
            Ok(Poll::Pending) => {
                match self.header().state.transition_to_idle() {
                    Ok(snapshot) => {
                        if snapshot.is_notified() {
                            // Signal yield
                            self.core().yield_now(Notified(self.to_task()));
                            // The ref-count was incremented as part of
                            // `transition_to_idle`.
                            self.drop_reference();
                        }
                    }
                    Err(_) => self.cancel_task(),
                }
            }
            Err(err) => {
                self.complete(Err(JoinError::panic2(err)), snapshot.is_join_interested());
            }
        }
    }

    pub(super) fn dealloc(self) {
        // Release the join waker, if there is one.
        self.trailer().waker.with_mut(|_| ());

        // Check causality
        self.core().stage.with_mut(|_| {});
        self.core().scheduler.with_mut(|_| {});

        unsafe {
            drop(Box::from_raw(self.cell.as_ptr()));
        }
    }

    // ===== join handle =====

    /// Read the task output into `dst`.
    pub(super) fn try_read_output(self, dst: &mut Poll<super::Result<T::Output>>, waker: &Waker) {
        // Load a snapshot of the current task state
        let snapshot = self.header().state.load();

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
                    self.trailer()
                        .waker
                        .with(|ptr| (*ptr).as_ref().unwrap().will_wake(waker))
                };

                if will_wake {
                    // The task is not complete **and** the waker is up to date,
                    // there is nothing further that needs to be done.
                    return;
                }

                // Unset the `JOIN_WAKER` to gain mutable access to the `waker`
                // field then update the field with the new join worker.
                //
                // This requires two atomic operations, unsetting the bit and
                // then resetting it. If the task transitions to complete
                // concurrently to either one of those operations, then setting
                // the join waker fails and we proceed to reading the task
                // output.
                self.header()
                    .state
                    .unset_waker()
                    .and_then(|snapshot| self.set_join_waker(waker.clone(), snapshot))
            } else {
                self.set_join_waker(waker.clone(), snapshot)
            };

            match res {
                Ok(_) => return,
                Err(snapshot) => {
                    assert!(snapshot.is_complete());
                }
            }
        }

        *dst = Poll::Ready(self.core().take_output());
    }

    fn set_join_waker(&self, waker: Waker, snapshot: Snapshot) -> Result<Snapshot, Snapshot> {
        assert!(snapshot.is_join_interested());
        assert!(!snapshot.has_join_waker());

        // Safety: Only the `JoinHandle` may set the `waker` field. When
        // `JOIN_INTEREST` is **not** set, nothing else will touch the field.
        unsafe {
            self.trailer().waker.with_mut(|ptr| {
                *ptr = Some(waker);
            });
        }

        // Update the `JoinWaker` state accordingly
        let res = self.header().state.set_join_waker();

        // If the state could not be updated, then clear the join waker
        if res.is_err() {
            unsafe {
                self.trailer().waker.with_mut(|ptr| {
                    *ptr = None;
                });
            }
        }

        res
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
            self.core().drop_future_or_output();
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
            self.core().schedule(Notified(self.to_task()));
        }
    }

    pub(super) fn drop_reference(self) {
        if self.header().state.ref_dec() {
            self.dealloc();
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
            return;
        }

        // By transitioning the lifcycle to `Running`, we have permission to
        // drop the future.
        self.cancel_task();
    }

    // ====== internal ======

    fn cancel_task(self) {
        // Drop the future from a panic guard.
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            self.core().drop_future_or_output();
        }));

        if let Err(err) = res {
            // Dropping the future panicked, complete the join
            // handle with the panic to avoid dropping the panic
            // on the ground.
            self.complete(Err(JoinError::panic2(err)), true);
        } else {
            self.complete(Err(JoinError::cancelled2()), true);
        }
    }

    fn complete(mut self, output: super::Result<T::Output>, is_join_interested: bool) {
        if is_join_interested {
            // Store the output. The future has already been dropped
            //
            // Safety: Mutual exclusion is obtained by having transitioned the task
            // state -> Running
            self.core().store_output(output);

            // Transition to `Complete`, notifying the `JoinHandle` if necessary.
            self.transition_to_complete();
        }

        // The task has completed execution and will no longer be scheduled.
        //
        // Attempts to batch a ref-dec with the state transition below.
        let ref_dec = if self.core().is_bound() {
            if let Some(task) = self.core().release(self.to_task()) {
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
            .header()
            .state
            .transition_to_terminal(!is_join_interested, ref_dec);

        if snapshot.ref_count() == 0 {
            self.dealloc()
        }
    }

    /// Transitions the task's lifecycle to `Complete`. Notifies the
    /// `JoinHandle` if it still has interest in the completion.
    fn transition_to_complete(&mut self) {
        // Transition the task's lifecycle to `Complete` and get a snapshot of
        // the task's sate.
        let snapshot = self.header().state.transition_to_complete();

        if !snapshot.is_join_interested() {
            // The `JoinHandle` is not interested in the output of this task. It
            // is our responsibility to drop the output.
            self.core().drop_future_or_output();
        } else if snapshot.has_join_waker() {
            // Notify the join handle. The previous transition obtains the
            // lock on the waker cell.
            self.wake_join();
        }
    }

    fn wake_join(&self) {
        self.trailer().waker.with(|ptr| match unsafe { &*ptr } {
            Some(waker) => waker.wake_by_ref(),
            None => panic!("waker missing"),
        });
    }

    fn to_task(&self) -> Task<S> {
        unsafe { Task::from_raw(self.header().into()) }
    }
}
