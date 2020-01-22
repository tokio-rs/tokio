//! An asynchronously awaitable WaitGroup which allows to wait for running tasks

use super::intrusive_double_linked_list::{LinkedList, ListNode};
use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

/// Updates a `Waker` which is stored inside a `Option` to the newest value
/// which is delivered via a `Context`.
fn update_waker_ref(waker_option: &mut Option<Waker>, cx: &Context<'_>) {
    if waker_option
        .as_ref()
        .map_or(true, |stored_waker| !stored_waker.will_wake(cx.waker()))
    {
        *waker_option = Some(cx.waker().clone());
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

/// Tracks the WaitGroupFuture waiting state.
/// Access to this struct is synchronized through the mutex in the WaitGroup.
struct WaitQueueEntry {
    /// The task handle of the waiting task
    task: Option<Waker>,
    /// Current polling state
    state: PollState,
}

impl WaitQueueEntry {
    /// Creates a new WaitQueueEntry
    fn new() -> WaitQueueEntry {
        WaitQueueEntry {
            task: None,
            state: PollState::New,
        }
    }
}

/// Internal state of the `WaitGroup`
struct GroupState {
    count: usize,
    waiters: LinkedList<WaitQueueEntry>,
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
        // This happens inside the lock to make cancellation reliable
        // If we would access waiters outside of the lock, the pointers
        // may no longer be valid.
        // Typically this shouldn't be an issue, since waking a task should
        // only move it from the blocked into the ready state and not have
        // further side effects.

        let waiters = self.waiters.take();

        unsafe {
            // Use a reverse iterator, so that the oldest waiter gets
            // scheduled first
            for waiter in waiters.into_reverse_iter() {
                if let Some(handle) = (*waiter).task.take() {
                    handle.wake();
                }
                (*waiter).state = PollState::Done;
            }
        }
    }

    /// Checks how many tasks are running. If none are running, this returns
    /// `Poll::Ready` immediately.
    /// If tasks are running, the WaitGroupFuture gets added to the wait
    /// queue at the group, and will be signalled once the tasks completed.
    /// This function is only safe as long as the `wait_node`s address is guaranteed
    /// to be stable until it gets removed from the queue.
    unsafe fn try_wait(
        &mut self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        match wait_node.state {
            PollState::New => {
                if self.count == 0 {
                    // The group is already signaled
                    wait_node.state = PollState::Done;
                    Poll::Ready(())
                } else {
                    // Added the task to the wait queue
                    wait_node.task = Some(cx.waker().clone());
                    wait_node.state = PollState::Waiting;
                    self.waiters.add_front(wait_node);
                    Poll::Pending
                }
            }
            PollState::Waiting => {
                // The WaitGroupFuture is already in the queue.
                // The group can't have reached 0 tasks, since this would change the
                // waitstate inside the mutex. However the caller might have
                // passed a different `Waker`. In this case we need to update it.
                update_waker_ref(&mut wait_node.task, cx);
                Poll::Pending
            }
            PollState::Done => {
                // We have been woken up by the group.
                // This does not guarantee that the group still has 0 running tasks.
                Poll::Ready(())
            }
        }
    }

    fn remove_waiter(&mut self, wait_node: &mut ListNode<WaitQueueEntry>) {
        // WaitGroupFuture only needs to get removed if it has been added to
        // the wait queue of the WaitGroup. This has happened in the PollState::Waiting case.
        if let PollState::Waiting = wait_node.state {
            if !unsafe { self.waiters.remove(wait_node) } {
                // Panic if the address isn't found. This can only happen if the contract was
                // violated, e.g. the WaitQueueEntry got moved after the initial poll.
                panic!("Future could not be removed from wait queue");
            }
            wait_node.state = PollState::Done;
        }
    }
}

/// A synchronization primitive which allows to wait until all tracked tasks
/// have finished.
///
/// Tasks can wait for tracked tasks to finish by obtaining a Future via `wait`.
/// This Future will get fulfilled when no tasks are running anymore.
pub(crate) struct WaitGroup {
    inner: Mutex<GroupState>,
}

// The Group is can be sent to other threads as long as it's not borrowed
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
            wait_node: ListNode::new(WaitQueueEntry::new()),
        }
    }

    unsafe fn try_wait(
        &self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        self.inner.lock().unwrap().try_wait(wait_node, cx)
    }

    fn remove_waiter(&self, wait_node: &mut ListNode<WaitQueueEntry>) {
        self.inner.lock().unwrap().remove_waiter(wait_node)
    }
}

/// A Future that is resolved once the corresponding WaitGroup has reached
/// 0 active tasks.
#[must_use = "futures do nothing unless polled"]
pub(crate) struct WaitGroupFuture<'a> {
    /// The WaitGroup that is associated with this WaitGroupFuture
    group: Option<&'a WaitGroup>,
    /// Node for waiting at the group
    wait_node: ListNode<WaitQueueEntry>,
}

// Safety: Futures can be sent between threads as long as the underlying
// group is thread-safe (Sync), which allows to poll/register/unregister from
// a different thread.
unsafe impl<'a> Send for WaitGroupFuture<'a> {}

impl<'a> core::fmt::Debug for WaitGroupFuture<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitGroupFuture").finish()
    }
}

impl<'a> Future for WaitGroupFuture<'a> {
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

        let poll_res = unsafe { group.try_wait(&mut mut_self.wait_node, cx) };

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
            ev.remove_waiter(&mut self.wait_node);
        }
    }
}
