//! An asynchronously awaitable `CancellationToken`.
//! The token allows to signal a cancellation request to one or more tasks.
pub(crate) mod guard;

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::sync::intrusive_double_linked_list::{LinkedList, ListNode};

use core::future::Future;
use core::pin::Pin;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll, Waker};
use std::sync::Arc;

use guard::DropGuard;

/// A token which can be used to signal a cancellation request to one or more
/// tasks.
///
/// Tasks can call [`CancellationToken::cancelled()`] in order to
/// obtain a Future which will be resolved when cancellation is requested.
///
/// Cancellation can be requested through the [`CancellationToken::cancel`] method.
///
/// # Examples
///
/// ```no_run
/// use tokio::select;
/// use tokio_util::sync::CancellationToken;
///
/// #[tokio::main]
/// async fn main() {
///     let token = CancellationToken::new();
///     let cloned_token = token.clone();
///
///     let join_handle = tokio::spawn(async move {
///         // Wait for either cancellation or a very long time
///         select! {
///             _ = cloned_token.cancelled() => {
///                 // The token was cancelled
///                 5
///             }
///             _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
///                 99
///             }
///         }
///     });
///
///     tokio::spawn(async move {
///         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
///         token.cancel();
///     });
///
///     assert_eq!(5, join_handle.await.unwrap());
/// }
/// ```
pub struct CancellationToken {
    inner: Arc<implementation::TreeNode>,
}

/*
/// A Future that is resolved once the corresponding [`CancellationToken`]
/// was cancelled
#[must_use = "futures do nothing unless polled"]
pub struct WaitForCancellationFuture<'a> {
    /// The CancellationToken that is associated with this WaitForCancellationFuture
    cancellation_token: Option<&'a CancellationToken>,
    /// Node for waiting at the cancellation_token
    wait_node: ListNode<WaitQueueEntry>,
    /// Whether this future was registered at the token yet as a waiter
    is_registered: bool,
}

// Safety: Futures can be sent between threads as long as the underlying
// cancellation_token is thread-safe (Sync),
// which allows to poll/register/unregister from a different thread.
unsafe impl<'a> Send for WaitForCancellationFuture<'a> {}
*/
// ===== impl CancellationToken =====

impl core::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

impl Clone for CancellationToken {
    fn clone(&self) -> Self {
        implementation::increase_handle_refcount(&self.inner);
        CancellationToken { inner: self.inner }
    }
}

impl Drop for CancellationToken {
    fn drop(&mut self) {
        implementation::decrease_handle_refcount(&self.inner);
    }
}

impl Default for CancellationToken {
    fn default() -> CancellationToken {
        CancellationToken::new()
    }
}

impl CancellationToken {
    /// Creates a new CancellationToken in the non-cancelled state.
    pub fn new() -> CancellationToken {
        CancellationToken {
            inner: Arc::new(implementation::TreeNode::new()),
        }
    }

    /// Creates a `CancellationToken` which will get cancelled whenever the
    /// current token gets cancelled.
    ///
    /// If the current token is already cancelled, the child token will get
    /// returned in cancelled state.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::select;
    /// use tokio_util::sync::CancellationToken;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let token = CancellationToken::new();
    ///     let child_token = token.child_token();
    ///
    ///     let join_handle = tokio::spawn(async move {
    ///         // Wait for either cancellation or a very long time
    ///         select! {
    ///             _ = child_token.cancelled() => {
    ///                 // The token was cancelled
    ///                 5
    ///             }
    ///             _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
    ///                 99
    ///             }
    ///         }
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    ///         token.cancel();
    ///     });
    ///
    ///     assert_eq!(5, join_handle.await.unwrap());
    /// }
    /// ```
    pub fn child_token(&self) -> CancellationToken {
        todo!()
    }

    /// Cancel the [`CancellationToken`] and all child tokens which had been
    /// derived from it.
    ///
    /// This will wake up all tasks which are waiting for cancellation.
    pub fn cancel(&self) {
        todo!()
    }

    /// Returns `true` if the `CancellationToken` had been cancelled
    pub fn is_cancelled(&self) -> bool {
        implementation::is_cancelled(&self.inner)
    }
    /*
    /// Returns a `Future` that gets fulfilled when cancellation is requested.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        WaitForCancellationFuture {
            cancellation_token: Some(self),
            wait_node: ListNode::new(WaitQueueEntry::new()),
            is_registered: false,
        }
    }


    /// Creates a `DropGuard` for this token.
    ///
    /// Returned guard will cancel this token (and all its children) on drop
    /// unless disarmed.
    pub fn drop_guard(self) -> DropGuard {
        DropGuard { inner: Some(self) }
    }

    unsafe fn register(
        &self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        self.state().register(wait_node, cx)
    }

    fn check_for_cancellation(
        &self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        self.state().check_for_cancellation(wait_node, cx)
    }

    fn unregister(&self, wait_node: &mut ListNode<WaitQueueEntry>) {
        self.state().unregister(wait_node)
    }*/
}
/*
// ===== impl WaitForCancellationFuture =====

impl<'a> core::fmt::Debug for WaitForCancellationFuture<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitForCancellationFuture").finish()
    }
}

impl<'a> Future for WaitForCancellationFuture<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // Safety: We do not move anything out of `WaitForCancellationFuture`
        let mut_self: &mut WaitForCancellationFuture<'_> = unsafe { Pin::get_unchecked_mut(self) };

        let cancellation_token = mut_self
            .cancellation_token
            .expect("polled WaitForCancellationFuture after completion");

        let poll_res = if !mut_self.is_registered {
            // Safety: The `ListNode` is pinned through the Future,
            // and we will unregister it in `WaitForCancellationFuture::drop`
            // before the Future is dropped and the memory reference is invalidated.
            unsafe { cancellation_token.register(&mut mut_self.wait_node, cx) }
        } else {
            cancellation_token.check_for_cancellation(&mut mut_self.wait_node, cx)
        };

        if let Poll::Ready(()) = poll_res {
            // The cancellation_token was signalled
            mut_self.cancellation_token = None;
            // A signalled Token means the Waker won't be enqueued anymore
            mut_self.is_registered = false;
            mut_self.wait_node.task = None;
        } else {
            // This `Future` and its stored `Waker` stay registered at the
            // `CancellationToken`
            mut_self.is_registered = true;
        }

        poll_res
    }
}

impl<'a> Drop for WaitForCancellationFuture<'a> {
    fn drop(&mut self) {
        // If this WaitForCancellationFuture has been polled and it was added to the
        // wait queue at the cancellation_token, it must be removed before dropping.
        // Otherwise the cancellation_token would access invalid memory.
        if let Some(token) = self.cancellation_token {
            if self.is_registered {
                token.unregister(&mut self.wait_node);
            }
        }
    }
}

/// Tracks how the future had interacted with the [`CancellationToken`]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PollState {
    /// The task has never interacted with the [`CancellationToken`].
    New,
    /// The task was added to the wait queue at the [`CancellationToken`].
    Waiting,
    /// The task has been polled to completion.
    Done,
}

/// Tracks the WaitForCancellationFuture waiting state.
/// Access to this struct is synchronized through the mutex in the CancellationToken.
struct WaitQueueEntry {
    /// The task handle of the waiting task
    task: Option<Waker>,
    // Current polling state. This state is only updated inside the Mutex of
    // the CancellationToken.
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

struct SynchronizedState {
    waiters: LinkedList<WaitQueueEntry>,
    first_child: Option<NonNull<CancellationTokenState>>,
    is_cancelled: bool,
}

impl SynchronizedState {
    fn new() -> Self {
        Self {
            waiters: LinkedList::new(),
            first_child: None,
            is_cancelled: false,
        }
    }
}

/// Information embedded in child tokens which is synchronized through the Mutex
/// in their parent.
struct SynchronizedThroughParent {
    next_peer: Option<NonNull<CancellationTokenState>>,
    prev_peer: Option<NonNull<CancellationTokenState>>,
}

/// Possible states of a `CancellationToken`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CancellationState {
    NotCancelled = 0,
    Cancelling = 1,
    Cancelled = 2,
}

impl CancellationState {
    fn pack(self) -> usize {
        self as usize
    }

    fn unpack(value: usize) -> Self {
        match value {
            0 => CancellationState::NotCancelled,
            1 => CancellationState::Cancelling,
            2 => CancellationState::Cancelled,
            _ => unreachable!("Invalid value"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct StateSnapshot {
    /// The amount of references to this particular CancellationToken.
    /// `CancellationToken` structs hold these references to a `CancellationTokenState`.
    /// Also the state is referenced by the state of each child.
    refcount: usize,
    /// Whether the state is still referenced by it's parent and can therefore
    /// not be freed.
    has_parent_ref: bool,
    /// Whether the token is cancelled
    cancel_state: CancellationState,
}

impl StateSnapshot {
    /// Packs the snapshot into a `usize`
    fn pack(self) -> usize {
        self.refcount << 3 | if self.has_parent_ref { 4 } else { 0 } | self.cancel_state.pack()
    }

    /// Unpacks the snapshot from a `usize`
    fn unpack(value: usize) -> Self {
        let refcount = value >> 3;
        let has_parent_ref = value & 4 != 0;
        let cancel_state = CancellationState::unpack(value & 0x03);

        StateSnapshot {
            refcount,
            has_parent_ref,
            cancel_state,
        }
    }

    /// Whether this `CancellationTokenState` is still referenced by any
    /// `CancellationToken`.
    fn has_refs(&self) -> bool {
        self.refcount != 0 || self.has_parent_ref
    }
}

/// The maximum permitted amount of references to a CancellationToken. This
/// is derived from the intent to never use more than 32bit in the `Snapshot`.
const MAX_REFS: u32 = (std::u32::MAX - 7) >> 3;

/// Internal state of the `CancellationToken` pair above
struct CancellationTokenState {
    state: AtomicUsize,
    parent: Option<NonNull<CancellationTokenState>>,
    from_parent: SynchronizedThroughParent,
    synchronized: Mutex<SynchronizedState>,
}

impl CancellationTokenState {
    fn new(
        parent: Option<NonNull<CancellationTokenState>>,
        state: StateSnapshot,
    ) -> CancellationTokenState {
        CancellationTokenState {
            parent,
            from_parent: SynchronizedThroughParent {
                prev_peer: None,
                next_peer: None,
            },
            state: AtomicUsize::new(state.pack()),
            synchronized: Mutex::new(SynchronizedState::new()),
        }
    }

    /// Returns a snapshot of the current atomic state of the token
    fn snapshot(&self) -> StateSnapshot {
        StateSnapshot::unpack(self.state.load(Ordering::SeqCst))
    }

    fn atomic_update_state<F>(&self, mut current_state: StateSnapshot, func: F) -> StateSnapshot
    where
        F: Fn(StateSnapshot) -> StateSnapshot,
    {
        let mut current_packed_state = current_state.pack();
        loop {
            let next_state = func(current_state);
            match self.state.compare_exchange(
                current_packed_state,
                next_state.pack(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return next_state;
                }
                Err(actual) => {
                    current_packed_state = actual;
                    current_state = StateSnapshot::unpack(actual);
                }
            }
        }
    }

    fn increment_refcount(&self, current_state: StateSnapshot) -> StateSnapshot {
        self.atomic_update_state(current_state, |mut state: StateSnapshot| {
            if state.refcount >= MAX_REFS as usize {
                eprintln!("[ERROR] Maximum reference count for CancellationToken was exceeded");
                std::process::abort();
            }
            state.refcount += 1;
            state
        })
    }

    fn decrement_refcount(&self, current_state: StateSnapshot) -> StateSnapshot {
        let current_state = self.atomic_update_state(current_state, |mut state: StateSnapshot| {
            state.refcount -= 1;
            state
        });

        // Drop the State if it is not referenced anymore
        if !current_state.has_refs() {
            // Safety: `CancellationTokenState` is always stored in refcounted
            // Boxes
            let _ = unsafe { Box::from_raw(self as *const Self as *mut Self) };
        }

        current_state
    }

    fn remove_parent_ref(&self, current_state: StateSnapshot) -> StateSnapshot {
        let current_state = self.atomic_update_state(current_state, |mut state: StateSnapshot| {
            state.has_parent_ref = false;
            state
        });

        // Drop the State if it is not referenced anymore
        if !current_state.has_refs() {
            // Safety: `CancellationTokenState` is always stored in refcounted
            // Boxes
            let _ = unsafe { Box::from_raw(self as *const Self as *mut Self) };
        }

        current_state
    }

    /// Unregisters a child from the parent token.
    /// The child tokens state is not exactly known at this point in time.
    /// If the parent token is cancelled, the child token gets removed from the
    /// parents list, and might therefore already have been freed. If the parent
    /// token is not cancelled, the child token is still valid.
    fn unregister_child(
        &mut self,
        mut child_state: NonNull<CancellationTokenState>,
        current_child_state: StateSnapshot,
    ) {
        let removed_child = {
            // Remove the child toke from the parents linked list
            let mut guard = self.synchronized.lock().unwrap();
            if !guard.is_cancelled {
                // Safety: Since the token was not cancelled, the child must
                // still be in the list and valid.
                let mut child_state = unsafe { child_state.as_mut() };
                debug_assert!(child_state.snapshot().has_parent_ref);

                if guard.first_child == Some(child_state.into()) {
                    guard.first_child = child_state.from_parent.next_peer;
                }
                // Safety: If peers wouldn't be valid anymore, they would try
                // to remove themselves from the list. This would require locking
                // the Mutex that we currently own.
                unsafe {
                    if let Some(mut prev_peer) = child_state.from_parent.prev_peer {
                        prev_peer.as_mut().from_parent.next_peer =
                            child_state.from_parent.next_peer;
                    }
                    if let Some(mut next_peer) = child_state.from_parent.next_peer {
                        next_peer.as_mut().from_parent.prev_peer =
                            child_state.from_parent.prev_peer;
                    }
                }
                child_state.from_parent.prev_peer = None;
                child_state.from_parent.next_peer = None;

                // The child is no longer referenced by the parent, since we were able
                // to remove its reference from the parents list.
                true
            } else {
                // Do not touch the linked list anymore. If the parent is cancelled
                // it will move all childs outside of the Mutex and manipulate
                // the pointers there. Manipulating the pointers here too could
                // lead to races. Therefore leave them just as as and let the
                // parent deal with it. The parent will make sure to retain a
                // reference to this state as long as it manipulates the list
                // pointers. Therefore the pointers are not dangling.
                false
            }
        };

        if removed_child {
            // If the token removed itself from the parents list, it can reset
            // the parent ref status. If it is isn't able to do so, because the
            // parent removed it from the list, there is no need to do this.
            // The parent ref acts as as another reference count. Therefore
            // removing this reference can free the object.
            // Safety: The token was in the list. This means the parent wasn't
            // cancelled before, and the token must still be alive.
            unsafe { child_state.as_mut().remove_parent_ref(current_child_state) };
        }

        // Decrement the refcount on the parent and free it if necessary
        self.decrement_refcount(self.snapshot());
    }

    fn cancel(&self) {
        // Move the state of the CancellationToken from `NotCancelled` to `Cancelling`
        let mut current_state = self.snapshot();

        let state_after_cancellation = loop {
            if current_state.cancel_state != CancellationState::NotCancelled {
                // Another task already initiated the cancellation
                return;
            }

            let mut next_state = current_state;
            next_state.cancel_state = CancellationState::Cancelling;
            match self.state.compare_exchange(
                current_state.pack(),
                next_state.pack(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break next_state,
                Err(actual) => current_state = StateSnapshot::unpack(actual),
            }
        };

        // This task cancelled the token

        // Take the task list out of the Token
        // We do not want to cancel child token inside this lock. If one of the
        // child tasks would have additional child tokens, we would recursively
        // take locks.

        // Doing this action has an impact if the child token is dropped concurrently:
        // It will try to deregister itself from the parent task, but can not find
        // itself in the task list anymore. Therefore it needs to assume the parent
        // has extracted the list and will process it. It may not modify the list.
        // This is OK from a memory safety perspective, since the parent still
        // retains a reference to the child task until it finished iterating over
        // it.

        let mut first_child = {
            let mut guard = self.synchronized.lock().unwrap();
            // Save the cancellation also inside the Mutex
            // This allows child tokens which want to detach themselves to detect
            // that this is no longer required since the parent cleared the list.
            guard.is_cancelled = true;

            // Wakeup all waiters
            // This happens inside the lock to make cancellation reliable
            // If we would access waiters outside of the lock, the pointers
            // may no longer be valid.
            // Typically this shouldn't be an issue, since waking a task should
            // only move it from the blocked into the ready state and not have
            // further side effects.

            // Use a reverse iterator, so that the oldest waiter gets
            // scheduled first
            guard.waiters.reverse_drain(|waiter| {
                // We are not allowed to move the `Waker` out of the list node.
                // The `Future` relies on the fact that the old `Waker` stays there
                // as long as the `Future` has not completed in order to perform
                // the `will_wake()` check.
                // Therefore `wake_by_ref` is used instead of `wake()`
                if let Some(handle) = &mut waiter.task {
                    handle.wake_by_ref();
                }
                // Mark the waiter to have been removed from the list.
                waiter.state = PollState::Done;
            });

            guard.first_child.take()
        };

        while let Some(mut child) = first_child {
            // Safety: We know this is a valid pointer since it is in our child pointer
            // list. It can't have been freed in between, since we retain a a reference
            // to each child.
            let mut_child = unsafe { child.as_mut() };

            // Get the next child and clean up list pointers
            first_child = mut_child.from_parent.next_peer;
            mut_child.from_parent.prev_peer = None;
            mut_child.from_parent.next_peer = None;

            // Cancel the child task
            mut_child.cancel();

            // Drop the parent reference. This `CancellationToken` is not interested
            // in interacting with the child anymore.
            // This is ONLY allowed once we promised not to touch the state anymore
            // after this interaction.
            mut_child.remove_parent_ref(mut_child.snapshot());
        }

        // The cancellation has completed
        // At this point in time tasks which registered a wait node can be sure
        // that this wait node already had been dequeued from the list without
        // needing to inspect the list.
        self.atomic_update_state(state_after_cancellation, |mut state| {
            state.cancel_state = CancellationState::Cancelled;
            state
        });
    }

    /// Returns `true` if the `CancellationToken` had been cancelled
    fn is_cancelled(&self) -> bool {
        let current_state = self.snapshot();
        current_state.cancel_state != CancellationState::NotCancelled
    }

    /// Registers a waiting task at the `CancellationToken`.
    /// Safety: This method is only safe as long as the waiting waiting task
    /// will properly unregister the wait node before it gets moved.
    unsafe fn register(
        &self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        debug_assert_eq!(PollState::New, wait_node.state);
        let current_state = self.snapshot();

        // Perform an optimistic cancellation check before. This is not strictly
        // necessary since we also check for cancellation in the Mutex, but
        // reduces the necessary work to be performed for tasks which already
        // had been cancelled.
        if current_state.cancel_state != CancellationState::NotCancelled {
            return Poll::Ready(());
        }

        // So far the token is not cancelled. However it could be cancelled before
        // we get the chance to store the `Waker`. Therefore we need to check
        // for cancellation again inside the mutex.
        let mut guard = self.synchronized.lock().unwrap();
        if guard.is_cancelled {
            // Cancellation was signalled
            wait_node.state = PollState::Done;
            Poll::Ready(())
        } else {
            // Added the task to the wait queue
            wait_node.task = Some(cx.waker().clone());
            wait_node.state = PollState::Waiting;
            guard.waiters.add_front(wait_node);
            Poll::Pending
        }
    }

    fn check_for_cancellation(
        &self,
        wait_node: &mut ListNode<WaitQueueEntry>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        debug_assert!(
            wait_node.task.is_some(),
            "Method can only be called after task had been registered"
        );

        let current_state = self.snapshot();

        if current_state.cancel_state != CancellationState::NotCancelled {
            // If the cancellation had been fully completed we know that our `Waker`
            // is no longer registered at the `CancellationToken`.
            // Otherwise the cancel call may or may not yet have iterated
            // through the waiters list and removed the wait nodes.
            // If it hasn't yet, we need to remove it. Otherwise an attempt to
            // reuse the `wait_node´ might get freed due to the `WaitForCancellationFuture`
            // getting dropped before the cancellation had interacted with it.
            if current_state.cancel_state != CancellationState::Cancelled {
                self.unregister(wait_node);
            }
            Poll::Ready(())
        } else {
            // Check if we need to swap the `Waker`. This will make the check more
            // expensive, since the `Waker` is synchronized through the Mutex.
            // If we don't need to perform a `Waker` update, an atomic check for
            // cancellation is sufficient.
            let need_waker_update = wait_node
                .task
                .as_ref()
                .map(|waker| !waker.will_wake(cx.waker()))
                .unwrap_or(true);

            if need_waker_update {
                let guard = self.synchronized.lock().unwrap();
                if guard.is_cancelled {
                    // Cancellation was signalled. Since this cancellation signal
                    // is set inside the Mutex, the old waiter must already have
                    // been removed from the waiting list
                    debug_assert_eq!(PollState::Done, wait_node.state);
                    wait_node.task = None;
                    Poll::Ready(())
                } else {
                    // The WaitForCancellationFuture is already in the queue.
                    // The CancellationToken can't have been cancelled,
                    // since this would change the is_cancelled flag inside the mutex.
                    // Therefore we just have to update the Waker. A follow-up
                    // cancellation will always use the new waker.
                    wait_node.task = Some(cx.waker().clone());
                    Poll::Pending
                }
            } else {
                // Do nothing. If the token gets cancelled, this task will get
                // woken again and can fetch the cancellation.
                Poll::Pending
            }
        }
    }

    fn unregister(&self, wait_node: &mut ListNode<WaitQueueEntry>) {
        debug_assert!(
            wait_node.task.is_some(),
            "waiter can not be active without task"
        );

        let mut guard = self.synchronized.lock().unwrap();
        // WaitForCancellationFuture only needs to get removed if it has been added to
        // the wait queue of the CancellationToken.
        // This has happened in the PollState::Waiting case.
        if let PollState::Waiting = wait_node.state {
            // Safety: Due to the state, we know that the node must be part
            // of the waiter list
            if !unsafe { guard.waiters.remove(wait_node) } {
                // Panic if the address isn't found. This can only happen if the contract was
                // violated, e.g. the WaitQueueEntry got moved after the initial poll.
                panic!("Future could not be removed from wait queue");
            }
            wait_node.state = PollState::Done;
        }
        wait_node.task = None;
    }
}
*/

/// GENERAL NOTES
/// -------------
///
/// **Locking**
///
/// Locking always needs to be performed top-down, to prevent deadlocks.
/// In detail, that means: If a both the parent and a child needs to be locked,
/// make sure to lock the parent first, and then the child.
///
/// **Consistency**
///
/// It is important that every call leaves the global state consistent.
/// No call should require further actions to restore consistency.
///
/// Example: The call to remove the reference to the parent of a child should
/// also remove the reference to the child from the parent.
///
///
mod implementation {
    use std::sync::{Arc, Mutex, MutexGuard};

    /// A node of the cancellation tree structure
    ///
    /// The actual data it holds is wrapped inside a mutex for synchronization.
    pub struct TreeNode {
        inner: Mutex<Inner>,
        waker: tokio::sync::Notify,
    }
    impl TreeNode {
        pub fn new() -> Self {
            Self {
                inner: Mutex::new(Inner {
                    parent: None,
                    parent_idx: 0,
                    children: vec![],
                    is_cancelled: false,
                    num_handles: 1,
                }),
                waker: tokio::sync::Notify::new(),
            }
        }
    }

    /// The data contained inside a TreeNode.
    ///
    /// This struct exists so that the data of the node can be wrapped
    /// in a Mutex.
    struct Inner {
        parent: Option<Arc<TreeNode>>,
        parent_idx: usize,
        children: Vec<Arc<TreeNode>>,
        is_cancelled: bool,
        num_handles: usize,
    }

    /// Returns whether or not the node is cancelled
    pub fn is_cancelled(node: &Arc<TreeNode>) -> bool {
        node.inner.lock().unwrap().is_cancelled
    }

    /// Disconnects the given parent from all of its children.
    ///
    /// Takes a reference to [Inner] to make sure the parent is already locked.
    ///
    /// Returns the (now parentless) children.
    fn disconnect_children(node: &mut Inner) -> Vec<Arc<TreeNode>> {
        for child in node.children {
            let child = child.inner.lock().unwrap();
            child.parent_idx = 0;
            child.parent = None;
        }

        let dropped_children = vec![];
        std::mem::swap(&mut node.children, &mut dropped_children);

        dropped_children
    }

    /// Figures out the parent of the node and locks the node and its parent atomically.
    ///
    /// The basic principle of preventing deadlocks in the tree is
    /// that we always lock the parent first, and then the child.
    ///
    /// Sadly, it's impossible to figure out the parent of a node without
    /// locking it. To then achieve locking order consistency, the node
    /// has to be unlocked before the parent gets locked.
    /// This leaves a small window where we already assume that we know the parent,
    /// but neighter the parent or the node is locked. Therefore, the parent could change.
    ///
    /// To prevent that this problem leaks into the rest of the code, it is abstracted
    /// in this function.
    fn lock_node_and_parent(
        node: &Arc<TreeNode>,
    ) -> (MutexGuard<'_, Inner>, Option<MutexGuard<'_, Inner>>) {
        loop {
            let potential_parent = {
                let locked_node = node.inner.lock().unwrap();
                match locked_node.parent {
                    Some(parent) => parent,
                    // If we locked the node and its parent is `None`, we are in a valid state
                    // and can return.
                    None => return (locked_node, None),
                }
            };

            // Deadlock safety:
            // This might deadlock if `node` suddenly became a PARENT of `potential_parent`.
            // This is impossible, though, as at most it might become a sibling or an uncle.
            let locked_parent = potential_parent.inner.lock().unwrap();
            let locked_node = node.inner.lock().unwrap();

            let actual_parent = match locked_node.parent {
                Some(parent) => parent,
                // If we locked the node and its parent is `None`, we are in a valid state
                // and can return.
                None => return (locked_node, None),
            };

            // Loop until we managed to lock both the node and its parent
            if Arc::ptr_eq(&actual_parent, &potential_parent) {
                return (locked_node, Some(locked_parent));
            }
        }
    }

    /// Moves all children from `node` to `parent`.
    ///
    /// `parent` MUST be the parent of `node`.
    /// To aquire the locks for node and parent, use `lock_node_and_parent`.
    fn move_children_to_parent(node: &mut Inner, parent: &mut Inner) {
        // Pre-allocate in the parent, for performance
        parent.children.reserve(node.children.len());

        for child in node.children.drain(..) {
            let child_locked = child.inner.lock().unwrap();
            child_locked.parent = node.parent;
            child_locked.parent_idx = parent.children.len();
            parent.children.push(child);
        }
    }

    pub fn increase_handle_refcount(node: &Arc<TreeNode>) {
        todo!();
    }

    pub fn decrease_handle_refcount(node: &Arc<TreeNode>) {
        todo!();
    }
}
