//! An asynchronously awaitable event for signalization between tasks

use super::intrusive_double_linked_list::{LinkedList, ListNode};
use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

/// Tracks how the future had interacted with the event
#[derive(PartialEq)]
enum PollState {
    /// The task has never interacted with the event.
    New,
    /// The task was added to the wait queue at the event.
    Waiting,
    /// The task has been polled to completion.
    Done,
}

/// Tracks the WaitForCancellationFuture waiting state.
/// Access to this struct is synchronized through the mutex in the Event.
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

/// Internal state of the `CancellationToken` pair above
struct CancellationTokenState {
    is_cancelled: bool,
    waiters: LinkedList<WaitQueueEntry>,
}

impl CancellationTokenState {
    fn new(is_cancelled: bool) -> CancellationTokenState {
        CancellationTokenState {
            is_cancelled,
            waiters: LinkedList::new(),
        }
    }

    fn cancel(&mut self) {
        if self.is_cancelled != true {
            self.is_cancelled = true;

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
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }

    /// Checks if the cancellation has occured. If it is this returns immediately.
    /// If the event isn't set, the WaitForCancellationFuture gets added to the wait
    /// queue at the event, and will be signalled once ready.
    /// This function is only safe as long as the `wait_node`s address is guaranteed
    /// to be stable until it gets removed from the queue.
    unsafe fn try_wait(
        &mut self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        match wait_node.state {
            PollState::New => {
                if self.is_cancelled {
                    // The event is already signaled
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
                // The WaitForCancellationFuture is already in the queue.
                // The event can't have been set, since this would change the
                // waitstate inside the mutex.
                // We don't need to update the Waker here, since we assume the
                // Future is only ever polled from the same task. If this wouldn't
                // hold, true, this wouldn't be safe.
                Poll::Pending
            }
            PollState::Done => {
                // We have been woken up by the event.
                // This does not guarantee that the event is still set. It could
                // have been reset it in the meantime.
                Poll::Ready(())
            }
        }
    }

    fn remove_waiter(&mut self, wait_node: &mut ListNode<WaitQueueEntry>) {
        // WaitForCancellationFuture only needs to get removed if it has been added to
        // the wait queue of the Event. This has happened in the PollState::Waiting case.
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

/// A synchronization primitive which can be either in the set or reset state.
///
/// Tasks can wait for the event to get set by obtaining a Future via `wait`.
/// This Future will get fulfilled when the event has been set.
pub(crate) struct CancellationToken {
    inner: Mutex<CancellationTokenState>,
}

// The Event is can be sent to other threads as long as it's not borrowed
unsafe impl Send for CancellationToken {}
// The Event is thread-safe as long as the utilized Mutex is thread-safe
unsafe impl Sync for CancellationToken {}

impl core::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CancellationToken").finish()
    }
}

impl CancellationToken {
    /// Creates a new CancellationToken in the given state
    pub(crate) fn new(is_cancelled: bool) -> CancellationToken {
        CancellationToken {
            inner: Mutex::new(CancellationTokenState::new(is_cancelled)),
        }
    }

    /// Sets the cancellation.
    ///
    /// Setting the cancellation will notify all pending waiters.
    pub(crate) fn cancel(&self) {
        self.inner.lock().unwrap().cancel()
    }

    /// Returns whether the CancellationToken is set
    pub(crate) fn is_cancelled(&self) -> bool {
        self.inner.lock().unwrap().is_cancelled()
    }

    /// Returns a future that gets fulfilled when the token is cancelled
    pub(crate) fn wait(&self) -> WaitForCancellationFuture<'_> {
        WaitForCancellationFuture {
            token: Some(self),
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

/// A Future that is resolved once the corresponding CancellationToken has been set
#[must_use = "futures do nothing unless polled"]
pub(crate) struct WaitForCancellationFuture<'a> {
    /// The CancellationToken that is associated with this WaitForCancellationFuture
    token: Option<&'a CancellationToken>,
    /// Node for waiting at the event
    wait_node: ListNode<WaitQueueEntry>,
}

unsafe impl<'a> Send for WaitForCancellationFuture<'a> {}

impl<'a> core::fmt::Debug for WaitForCancellationFuture<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitForCancellationFuture").finish()
    }
}

impl<'a> Future for WaitForCancellationFuture<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // It might be possible to use Pin::map_unchecked here instead of the two unsafe APIs.
        // However this didn't seem to work for some borrow checker reasons

        // Safety: The next operations are safe, because Pin promises us that
        // the address of the wait queue entry inside MutexLocalFuture is stable,
        // and we don't move any fields inside the future until it gets dropped.
        let mut_self: &mut WaitForCancellationFuture<'_> = unsafe { Pin::get_unchecked_mut(self) };

        let token = mut_self
            .token
            .expect("polled WaitForCancellationFuture after completion");

        let poll_res = unsafe { token.try_wait(&mut mut_self.wait_node, cx) };

        if let Poll::Ready(()) = poll_res {
            // The token was set
            mut_self.token = None;
        }

        poll_res
    }
}

impl<'a> Drop for WaitForCancellationFuture<'a> {
    fn drop(&mut self) {
        // If this WaitForCancellationFuture has been polled and it was added to the
        // wait queue at the event, it must be removed before dropping.
        // Otherwise the event would access invalid memory.
        if let Some(token) = self.token {
            token.remove_waiter(&mut self.wait_node);
        }
    }
}
