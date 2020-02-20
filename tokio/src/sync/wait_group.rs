//! An asynchronously awaitable WaitGroup which allows to wait for running tasks
//! to complete.

use crate::{
    loom::sync::{Arc, Mutex},
    util::linked_list::{self, LinkedList},
};
use std::{
    cell::UnsafeCell,
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll, Waker},
};

/// A synchronization primitive which allows to wait until all tracked tasks
/// have finished.
///
/// Tasks can wait for tracked tasks to finish by obtaining a Future via `wait`.
/// This Future will get fulfilled when no tasks are running anymore.
pub(crate) struct WaitGroup {
    inner: Mutex<GroupState>,
}

// The Group can be sent to other threads as long as it's not borrowed
unsafe impl Send for WaitGroup {}
// The Group is thread-safe as long as the utilized Mutex is thread-safe
unsafe impl Sync for WaitGroup {}

impl core::fmt::Debug for WaitGroup {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitGroup").finish()
    }
}

impl WaitGroup {
    /// Creates a new WaitGroup
    pub(crate) fn new() -> WaitGroup {
        WaitGroup {
            inner: Mutex::new(GroupState::new(0)),
        }
    }

    /// Adds a pending task to the WaitGroup
    pub(crate) fn add(&self) {
        self.inner.lock().unwrap().add()
    }

    /// Removes a task that has finished from the WaitGroup
    pub(crate) fn remove(&self) {
        self.inner.lock().unwrap().remove()
    }

    /// Returns a future that gets fulfilled when all tracked tasks complete
    pub(crate) fn wait(&self) -> WaitGroupFuture<'_> {
        WaitGroupFuture {
            group: Some(self),
            waiter: UnsafeCell::new(Waiter::new()),
        }
    }

    unsafe fn try_wait(&self, waiter: &mut UnsafeCell<Waiter>, cx: &mut Context<'_>) -> Poll<()> {
        let mut guard = self.inner.lock().unwrap();
        // Safety: The wait node is only accessed inside the Mutex
        let waiter = &mut *waiter.get();
        guard.try_wait(waiter, cx)
    }

    fn remove_waiter(&self, waiter: &mut UnsafeCell<Waiter>) {
        let mut guard = self.inner.lock().unwrap();
        // Safety: The wait node is only accessed inside the Mutex
        let waiter = unsafe { &mut *waiter.get() };
        guard.remove_waiter(waiter)
    }
}

/// A Future that is resolved once the corresponding WaitGroup has reached
/// 0 active tasks.
#[must_use = "futures do nothing unless polled"]
pub(crate) struct WaitGroupFuture<'a> {
    /// The WaitGroup that is associated with this WaitGroupFuture
    group: Option<&'a WaitGroup>,
    /// Node for waiting at the group
    waiter: UnsafeCell<Waiter>,
}

// Safety: Futures can be sent between threads, since the underlying
// group is thread-safe (Sync), which allows to poll/register/unregister from
// a different thread.
unsafe impl<'a> Send for WaitGroupFuture<'a> {}

impl<'a> core::fmt::Debug for WaitGroupFuture<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitGroupFuture").finish()
    }
}

impl Future for WaitGroupFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // It might be possible to use Pin::map_unchecked here instead of the two unsafe APIs.
        // However this didn't seem to work for some borrow checker reasons

        // Safety: The next operations are safe, because Pin promises us that
        // the address of the wait queue entry inside WaitGroupFuture is stable,
        // and we don't move any fields inside the future until it gets dropped.
        let mut_self: &mut WaitGroupFuture<'_> = unsafe { Pin::get_unchecked_mut(self) };

        let group = mut_self
            .group
            .expect("polled WaitGroupFuture after completion");

        let poll_res = unsafe { group.try_wait(&mut mut_self.waiter, cx) };

        if let Poll::Ready(()) = poll_res {
            mut_self.group = None;
        }

        poll_res
    }
}

impl<'a> Drop for WaitGroupFuture<'a> {
    fn drop(&mut self) {
        // If this WaitGroupFuture has been polled and it was added to the
        // wait queue at the group, it must be removed before dropping.
        // Otherwise the group would access invalid memory.
        if let Some(ev) = self.group {
            ev.remove_waiter(&mut self.waiter);
        }
    }
}

/// A cloneable [`WaitGroup`]
///
/// When tasks are added to this [`WaitGroup`] a [`WaitGroupReleaser`] will be
/// returned, which will automatically decrement the count of active tasks in
/// the [`SharedWaitGroup`] when dropped.
#[derive(Clone)]
pub(crate) struct SharedWaitGroup {
    inner: Arc<WaitGroup>,
}

impl SharedWaitGroup {
    /// Creates a new [`SharedWaitGroup`]
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(WaitGroup::new()),
        }
    }

    /// Registers a task at the [`SharedWaitGroup`]
    ///
    /// The method returns a [`WaitGroupReleaser`] which is intended to be dropped
    /// once the task completes.
    #[must_use]
    pub(crate) fn add(&self) -> WaitGroupReleaser {
        self.inner.add();
        WaitGroupReleaser {
            inner: self.inner.clone(),
        }
    }

    /// Returns a [`Future`] which will complete once all tasks which have been
    /// previously added have dropped their [`WaitGroupReleaser`] and are thereby
    /// deemed as finished.
    pub(crate) fn wait_future(&self) -> WaitGroupFuture<'_> {
        self.inner.wait()
    }
}

/// A handle which tracks an active task which is monitored by the [`SharedWaitGroup`].
/// When this object is dropped, the task will be automatically be marked as
/// completed inside the [`SharedWaitGroup`].
pub(crate) struct WaitGroupReleaser {
    inner: Arc<WaitGroup>,
}

impl Drop for WaitGroupReleaser {
    fn drop(&mut self) {
        self.inner.remove();
    }
}

/// Tracks how the future had interacted with the group
#[derive(PartialEq)]
enum PollState {
    /// The task has never interacted with the group.
    New,
    /// The task was added to the wait queue at the group.
    Waiting,
    /// The task has been polled to completion.
    Done,
}

/// A `Waiter` allows a task to wait o the `WaitGroup`. A `Waiter` is a node
/// in a linked list which is managed through the `WaitGroup`.
/// Access to this struct is synchronized through the mutex in the WaitGroup.
struct Waiter {
    /// Intrusive linked-list pointers
    pointers: linked_list::Pointers<Waiter>,
    /// The task handle of the waiting task
    waker: Option<Waker>,
    /// Current polling state
    state: PollState,
    /// Should not be `Unpin`.
    _p: PhantomPinned,
}

impl Waiter {
    /// Creates a new Waiter
    fn new() -> Waiter {
        Waiter {
            pointers: linked_list::Pointers::new(),
            waker: None,
            state: PollState::New,
            _p: PhantomPinned,
        }
    }
}

/// # Safety
///
/// `Waiter` is forced to be !Unpin.
unsafe impl linked_list::Link for Waiter {
    type Handle = NonNull<Waiter>;
    type Target = Waiter;

    fn as_raw(handle: &NonNull<Waiter>) -> NonNull<Waiter> {
        *handle
    }

    unsafe fn from_raw(ptr: NonNull<Waiter>) -> NonNull<Waiter> {
        ptr
    }

    unsafe fn pointers(mut target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
        NonNull::from(&mut target.as_mut().pointers)
    }
}

/// Internal state of the `WaitGroup`
struct GroupState {
    count: usize,
    waiters: LinkedList<Waiter>,
}

impl GroupState {
    fn new(count: usize) -> GroupState {
        GroupState {
            count,
            waiters: LinkedList::new(),
        }
    }

    fn add(&mut self) {
        self.count += 1;
    }

    fn remove(&mut self) {
        if self.count == 0 {
            return;
        }
        self.count -= 1;
        if self.count != 0 {
            return;
        }

        // Wakeup all waiters
        while let Some(mut waiter) = self.waiters.pop_back() {
            // Safety: waiters lock is held
            let waiter = unsafe { waiter.as_mut() };
            if let Some(handle) = (*waiter).waker.take() {
                handle.wake();
            }
            (*waiter).state = PollState::Done;
        }
    }

    /// Checks how many tasks are running. If none are running, this returns
    /// `Poll::Ready` immediately.
    /// If tasks are running, the WaitGroupFuture gets added to the wait
    /// queue at the group, and will be signalled once the tasks completed.
    /// This function is only safe as long as the `waiter`s address is guaranteed
    /// to be stable until it gets removed from the queue.
    unsafe fn try_wait(&mut self, waiter: &mut Waiter, cx: &mut Context<'_>) -> Poll<()> {
        match waiter.state {
            PollState::New => {
                if self.count == 0 {
                    // The group is already signaled
                    waiter.state = PollState::Done;
                    Poll::Ready(())
                } else {
                    // Added the task to the wait queue
                    waiter.waker = Some(cx.waker().clone());
                    waiter.state = PollState::Waiting;
                    self.waiters.push_front(waiter.into());
                    Poll::Pending
                }
            }
            PollState::Waiting => {
                // The WaitGroupFuture is already in the queue.
                // The group can't have reached 0 tasks, since this would change the
                // waitstate inside the mutex. However the caller might have
                // passed a different `Waker`. In this case we need to update it.
                if waiter
                    .waker
                    .as_ref()
                    .map_or(true, |stored_waker| !stored_waker.will_wake(cx.waker()))
                {
                    waiter.waker = Some(cx.waker().clone());
                }

                Poll::Pending
            }
            PollState::Done => {
                // We have been woken up by the group.
                // This does not guarantee that the group still has 0 running tasks.
                Poll::Ready(())
            }
        }
    }

    fn remove_waiter(&mut self, waiter: &mut Waiter) {
        // WaitGroupFuture only needs to get removed if it has been added to
        // the wait queue of the WaitGroup. This has happened in the PollState::Waiting case.
        if let PollState::Waiting = waiter.state {
            if unsafe { self.waiters.remove(waiter.into()).is_none() } {
                // Panic if the address isn't found. This can only happen if the contract was
                // violated, e.g. the Waiter got moved after the initial poll.
                panic!("Future could not be removed from wait queue");
            }
            waiter.state = PollState::Done;
        }
    }
}
