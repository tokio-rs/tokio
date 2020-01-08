use crate::loom::alloc::Track;
use crate::loom::cell::CausalCheck;
use crate::task::core::{Cell, Core, Header, Trailer};
use crate::task::state::Snapshot;
use crate::task::{JoinError, Schedule, Task};

use std::future::Future;
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ptr::NonNull;
use std::task::{Poll, Waker};

/// Typed raw task handle
pub(super) struct Harness<T: Future, S: 'static> {
    cell: NonNull<Cell<T>>,
    _p: PhantomData<S>,
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: 'static,
{
    pub(super) unsafe fn from_raw(ptr: *mut ()) -> Harness<T, S> {
        debug_assert!(!ptr.is_null());

        Harness {
            cell: NonNull::new_unchecked(ptr as *mut Cell<T>),
            _p: PhantomData,
        }
    }

    fn header(&self) -> &Header {
        unsafe { &self.cell.as_ref().header }
    }

    fn trailer(&self) -> &Trailer {
        unsafe { &self.cell.as_ref().trailer }
    }

    fn core(&mut self) -> &mut Core<T> {
        unsafe { &mut self.cell.as_mut().core }
    }
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: Schedule,
{
    /// Poll the inner future.
    ///
    /// All necessary state checks and transitions are performed.
    ///
    /// Panics raised while polling the future are handled.
    ///
    /// Returns `true` if the task needs to be scheduled again
    ///
    /// # Safety
    ///
    /// The pointer returned by the `executor` fn must be castable to `*mut S`
    pub(super) unsafe fn poll(mut self, executor: &mut dyn FnMut() -> Option<NonNull<()>>) -> bool {
        use std::panic;

        // Transition the task to the running state.
        let res = self.header().state.transition_to_running();

        if res.is_canceled() {
            // The task was concurrently canceled.
            self.do_cancel(res);
            return false;
        }

        let join_interest = res.is_join_interested();
        debug_assert!(join_interest || !res.has_join_waker());

        // Get the cell components
        let cell = &mut self.cell.as_mut();
        let header = &cell.header;
        let core = &mut cell.core;

        // If the task's executor pointer is not yet set, then set it here. This
        // is safe because a) this is the only time the value is set. b) at this
        // point, there are no outstanding wakers which might access the
        // field concurrently.
        if header.executor().is_none() {
            // We don't want the destructor to run because we don't really
            // own the task here.
            let task = ManuallyDrop::new(Task::from_raw(header.into()));
            // Call the scheduler's bind callback
            let executor = executor().expect("first poll must happen from an executor");
            executor.cast::<S>().as_ref().bind(&task);
            header.executor.with_mut(|ptr| *ptr = Some(executor.cast()));
        }

        // The transition to `Running` done above ensures that a lock on the
        // future has been obtained. This also ensures the `*mut T` pointer
        // contains the future (as opposed to the output) and is initialized.

        let res = header.future_causality.with_mut(|_| {
            panic::catch_unwind(panic::AssertUnwindSafe(|| {
                struct Guard<'a, T: Future> {
                    core: &'a mut Core<T>,
                    polled: bool,
                }

                impl<T: Future> Drop for Guard<'_, T> {
                    fn drop(&mut self) {
                        if !self.polled {
                            self.core.transition_to_consumed();
                        }
                    }
                }

                let mut guard = Guard {
                    core,
                    polled: false,
                };

                let res = guard.core.poll::<S>(header);

                // prevent the guard from dropping the future
                guard.polled = true;

                res
            }))
        });

        match res {
            Ok(Poll::Ready(out)) => {
                self.complete(executor, join_interest, Ok(out));
                false
            }
            Ok(Poll::Pending) => {
                let res = self.header().state.transition_to_idle();

                if res.is_canceled() {
                    self.do_cancel(res);
                    false
                } else {
                    res.is_notified()
                }
            }
            Err(err) => {
                self.complete(executor, join_interest, Err(JoinError::panic2(err)));
                false
            }
        }
    }

    pub(super) unsafe fn drop_task(mut self) {
        let might_drop_join_waker_on_release = self.might_drop_join_waker_on_release();

        // Read the join waker cell just to have it
        let (join_waker, check) = self.read_join_waker();

        // transition the task to released
        let res = self.header().state.release_task();

        assert!(res.is_terminal(), "state = {:?}", res);

        if might_drop_join_waker_on_release && !res.is_join_interested() {
            debug_assert!(res.has_join_waker());

            // Its our responsibility to drop the waker
            check.check();
            let _ = join_waker.assume_init();
        }

        if res.is_final_ref() {
            self.dealloc();
        }
    }

    unsafe fn dealloc(self) {
        // Check causality
        self.header().executor.with_mut(|_| {});
        self.header().future_causality.with_mut(|_| {});
        self.trailer().waker.with_mut(|_| {
            // we can't check the contents of this cell as it is considered
            // "uninitialized" data at this point.
        });

        drop(Box::from_raw(self.cell.as_ptr()));
    }

    // ===== join handle =====

    pub(super) unsafe fn read_output(
        mut self,
        dst: *mut Track<super::Result<T::Output>>,
        state: Snapshot,
    ) {
        if state.is_canceled() {
            dst.write(Track::new(Err(JoinError::cancelled2())));
        } else {
            self.core().read_output(dst);
        }

        // Before transitioning the state, the waker must be read. It is
        // possible that, after the transition, we are responsible for dropping
        // the waker but before the waker can be read from the struct, the
        // struct is deallocated.
        let (waker, check) = self.read_join_waker();

        // The operation counts as dropping the join handle
        let res = self.header().state.complete_join_handle();

        if res.is_released() {
            // We are responsible for freeing the waker handle
            check.check();
            drop(waker.assume_init());
        }

        if res.is_final_ref() {
            self.dealloc();
        }
    }

    pub(super) fn store_join_waker(&self, waker: &Waker) -> Snapshot {
        unsafe {
            self.trailer().waker.with_mut(|ptr| {
                (*ptr).as_mut_ptr().replace(Some(waker.clone()));
            });
        }

        let res = self.header().state.store_join_waker();

        if res.is_complete() || res.is_canceled() {
            // Drop the waker here
            self.trailer()
                .waker
                .with_mut(|ptr| unsafe { *(*ptr).as_mut_ptr() = None });
        }

        res
    }

    pub(super) fn swap_join_waker(&self, waker: &Waker, prev: Snapshot) -> Snapshot {
        unsafe {
            let will_wake = self
                .trailer()
                .waker
                .with(|ptr| (*(*ptr).as_ptr()).as_ref().unwrap().will_wake(waker));

            if will_wake {
                return prev;
            }

            // Acquire the lock
            let state = self.header().state.unset_waker();

            if state.is_active() {
                return self.store_join_waker(waker);
            }

            state
        }
    }

    pub(super) fn drop_join_handle_slow(mut self) {
        unsafe {
            // Before transitioning the state, the waker must be read. It is
            // possible that, after the transition, we are responsible for dropping
            // the waker but before the waker can be read from the struct, the
            // struct is deallocated.
            let (waker, check) = self.read_join_waker();

            // The operation counts as dropping the join handle
            let res = match self.header().state.drop_join_handle_slow() {
                Ok(res) => res,
                Err(res) => {
                    // The task output must be read & dropped
                    debug_assert!(!(res.is_complete() && res.is_canceled()));

                    if res.is_complete() {
                        self.core().transition_to_consumed();
                    }

                    self.header().state.complete_join_handle()
                }
            };

            if !(res.is_complete() | res.is_canceled()) || res.is_released() {
                // We are responsible for freeing the waker handle
                check.check();
                drop(waker.assume_init());
            }

            if res.is_final_ref() {
                self.dealloc();
            }
        }
    }

    // ===== waker behavior =====

    pub(super) fn wake_by_val(self) {
        self.wake_by_ref();
        self.drop_waker();
    }

    pub(super) fn wake_by_ref(&self) {
        if self.header().state.transition_to_notified() {
            unsafe {
                let executor = match self.header().executor.with(|ptr| *ptr) {
                    Some(executor) => executor,
                    None => panic!("executor should be set"),
                };

                S::schedule(executor.cast().as_ref(), self.to_task());
            }
        }
    }

    pub(super) fn drop_waker(self) {
        if self.header().state.ref_dec() {
            unsafe {
                self.dealloc();
            }
        }
    }

    /// Cancel the task.
    ///
    /// `from_queue` signals the caller is cancelling the task after popping it
    /// from the queue. This indicates "polling" capability.
    pub(super) fn cancel(self, from_queue: bool) {
        let res = if from_queue {
            self.header().state.transition_to_canceled_from_queue()
        } else {
            match self.header().state.transition_to_canceled_from_list() {
                Some(res) => res,
                None => return,
            }
        };

        self.do_cancel(res);
    }

    fn do_cancel(mut self, res: Snapshot) {
        use std::panic;

        debug_assert!(!res.is_complete());

        let cell = unsafe { &mut self.cell.as_mut() };
        let header = &cell.header;
        let core = &mut cell.core;

        // Since we transitioned the task state to `canceled`, it won't ever be
        // polled again. We are now responsible for all cleanup.
        //
        // We have to drop the future
        //
        header.future_causality.with_mut(|_| {
            // Guard against potential panics in the drop handler
            let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                // Drop the future
                core.transition_to_consumed();
            }));
        });

        // If there is a join waker, we must notify it so it can observe the
        // task was canceled.
        if res.is_join_interested() && res.has_join_waker() {
            // Notify the join handle. The transition to cancelled obtained a
            // lock on the waker cell.
            unsafe {
                self.wake_join();
            }

            // Also track that we might be responsible for releasing the waker.
            self.set_might_drop_join_waker_on_release();
        }

        // The `RELEASED` flag is not set yet.
        assert!(!res.is_final_ref());

        // This **can** be null if the task is being cancelled before it was
        // ever polled.
        let bound_executor = unsafe { self.header().executor.with(|ptr| *ptr) };

        unsafe {
            let task = self.to_task();

            if let Some(executor) = bound_executor {
                executor.cast::<S>().as_ref().release(task);
            } else {
                // Just drop the task. This will release / deallocate memory.
                drop(task);
            }
        }
    }

    // ====== internal ======

    fn complete(
        mut self,
        executor: &mut dyn FnMut() -> Option<NonNull<()>>,
        join_interest: bool,
        output: super::Result<T::Output>,
    ) {
        if join_interest {
            // Store the output. The future has already been dropped
            self.core().store_output(output);
        }

        let executor = executor();
        let bound_executor = unsafe { self.header().executor.with(|ptr| *ptr) };

        // Handle releasing the task. First, check if the current
        // executor is the one that is bound to the task:
        if executor.is_some() && executor == bound_executor {
            unsafe {
                // perform a local release
                let task = ManuallyDrop::new(self.to_task());
                executor
                    .as_ref()
                    .unwrap()
                    .cast::<S>()
                    .as_ref()
                    .release_local(&task);

                if self.transition_to_released(join_interest).is_final_ref() {
                    self.dealloc();
                }
            }
        } else {
            let res = self.transition_to_complete(join_interest);
            assert!(!res.is_final_ref());

            if res.has_join_waker() {
                // The release step happens later once the task has migrated back to
                // the worker that owns it. At that point, the releaser **may** also
                // be responsible for dropping. This fact must be tracked until
                // the release step happens.
                self.set_might_drop_join_waker_on_release();
            }

            unsafe {
                let task = self.to_task();

                let executor = match bound_executor {
                    Some(executor) => executor,
                    None => panic!("executor should be set"),
                };

                executor.cast::<S>().as_ref().release(task);
            }
        }
    }

    /// Return `true` if the task structure should be deallocated
    fn transition_to_complete(&mut self, join_interest: bool) -> Snapshot {
        let res = self.header().state.transition_to_complete();

        self.notify_join_handle(join_interest, res);

        // Transition to complete last to ensure freeing does
        // not happen until the above work is done.
        res
    }

    /// Return `true` if the task structure should be deallocated
    fn transition_to_released(&mut self, join_interest: bool) -> Snapshot {
        if join_interest {
            let res1 = self.transition_to_complete(join_interest);

            // At this point, the join waker may not be changed. Once we perform
            // `release_task` we may no longer read from the struct but we
            // **may** be responsible for dropping the waker. We do an
            // optimistic read here.
            let (join_waker, check) = unsafe { self.read_join_waker() };

            let res2 = self.header().state.release_task();

            if res1.has_join_waker() && !res2.is_join_interested() {
                debug_assert!(res2.has_join_waker());

                // Its our responsibility to drop the waker
                check.check();
                unsafe {
                    drop(join_waker.assume_init());
                }
            }

            res2
        } else {
            self.header().state.transition_to_released()
        }
    }

    fn notify_join_handle(&mut self, join_interest: bool, res: Snapshot) {
        if join_interest {
            if !res.is_join_interested() {
                debug_assert!(!res.has_join_waker());

                // The join handle dropped interest before we could release
                // the output. We are now responsible for releasing the
                // output.
                self.core().transition_to_consumed();
            } else if res.has_join_waker() {
                if res.is_canceled() {
                    // The join handle will set the output to Cancelled without
                    // attempting to read the output. We must drop it here.
                    self.core().transition_to_consumed();
                }

                // Notify the join handle. The previous transition obtains the
                // lock on the waker cell.
                unsafe {
                    self.wake_join();
                }
            }
        }
    }

    fn might_drop_join_waker_on_release(&self) -> bool {
        unsafe {
            let next = *self.header().queue_next.get() as usize;
            next & 1 == 1
        }
    }

    fn set_might_drop_join_waker_on_release(&self) {
        unsafe {
            debug_assert!(
                (*self.header().queue_next.get()).is_null(),
                "the task's queue_next field must be null when releasing"
            );

            *self.header().queue_next.get() = 1 as *const _;
        }
    }

    unsafe fn wake_join(&self) {
        // LOOM: ensure we can make this call
        self.trailer().waker.check();
        self.trailer().waker.with_unchecked(|ptr| {
            (*(*ptr).as_ptr())
                .as_ref()
                .expect("waker missing")
                .wake_by_ref();
        });
    }

    unsafe fn read_join_waker(&mut self) -> (MaybeUninit<Option<Waker>>, CausalCheck) {
        self.trailer().waker.with_deferred(|ptr| ptr.read())
    }

    unsafe fn to_task(&self) -> Task<S> {
        let ptr = self.cell.as_ptr() as *mut Header;
        Task::from_raw(NonNull::new_unchecked(ptr))
    }
}
