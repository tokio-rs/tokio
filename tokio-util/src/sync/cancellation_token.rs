//! An asynchronously awaitable `CancellationToken`.
//! The token allows to signal a cancellation request to one or more tasks.

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::sync::intrusive_double_linked_list::{LinkedList, ListNode};

use core::future::Future;
use core::pin::Pin;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll, Waker};

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
/// ```ignore
/// use tokio::select;
/// use tokio::scope::CancellationToken;
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
    inner: NonNull<CancellationTokenState>,
}

// Safety: The CancellationToken is thread-safe and can be moved between threads,
// since all methods are internally synchronized.
unsafe impl Send for CancellationToken {}
unsafe impl Sync for CancellationToken {}

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
        // Safety: The state inside a `CancellationToken` is always valid, since
        // is reference counted
        let inner = self.state();

        // Tokens are cloned by increasing their refcount
        let current_state = inner.snapshot();
        inner.increment_refcount(current_state);

        CancellationToken { inner: self.inner }
    }
}

impl Drop for CancellationToken {
    fn drop(&mut self) {
        let token_state_pointer = self.inner;

        // Safety: The state inside a `CancellationToken` is always valid, since
        // is reference counted
        let inner = unsafe { &mut *self.inner.as_ptr() };

        let mut current_state = inner.snapshot();

        // We need to safe the parent, since the state might be released by the
        // next call
        let parent = inner.parent;

        // Drop our own refcount
        current_state = inner.decrement_refcount(current_state);

        // If this was the last reference, unregister from the parent
        if current_state.refcount == 0 {
            if let Some(mut parent) = parent {
                // Safety: Since we still retain a reference on the parent, it must be valid.
                let parent = unsafe { parent.as_mut() };
                parent.unregister_child(token_state_pointer, current_state);
            }
        }
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
        let state = Box::new(CancellationTokenState::new(
            None,
            StateSnapshot {
                cancel_state: CancellationState::NotCancelled,
                has_parent_ref: false,
                refcount: 1,
            },
        ));

        // Safety: We just created the Box. The pointer is guaranteed to be
        // not null
        CancellationToken {
            inner: unsafe { NonNull::new_unchecked(Box::into_raw(state)) },
        }
    }

    /// Returns a reference to the utilized `CancellationTokenState`.
    fn state(&self) -> &CancellationTokenState {
        // Safety: The state inside a `CancellationToken` is always valid, since
        // is reference counted
        unsafe { &*self.inner.as_ptr() }
    }

    /// Creates a `CancellationToken` which will get cancelled whenever the
    /// current token gets cancelled.
    ///
    /// If the current token is already cancelled, the child token will get
    /// returned in cancelled state.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use tokio::select;
    /// use tokio::scope::CancellationToken;
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
        let inner = self.state();

        // Increment the refcount of this token. It will be referenced by the
        // child, independent of whether the child is immediately cancelled or
        // not.
        let _current_state = inner.increment_refcount(inner.snapshot());

        let mut unpacked_child_state = StateSnapshot {
            has_parent_ref: true,
            refcount: 1,
            cancel_state: CancellationState::NotCancelled,
        };
        let mut child_token_state = Box::new(CancellationTokenState::new(
            Some(self.inner),
            unpacked_child_state,
        ));

        {
            let mut guard = inner.synchronized.lock().unwrap();
            if guard.is_cancelled {
                // This task was already cancelled. In this case we should not
                // insert the child into the list, since it would never get removed
                // from the list.
                (*child_token_state.synchronized.lock().unwrap()).is_cancelled = true;
                unpacked_child_state.cancel_state = CancellationState::Cancelled;
                // Since it's not in the list, the parent doesn't need to retain
                // a reference to it.
                unpacked_child_state.has_parent_ref = false;
                child_token_state
                    .state
                    .store(unpacked_child_state.pack(), Ordering::SeqCst);
            } else {
                if let Some(mut first_child) = guard.first_child {
                    child_token_state.from_parent.next_peer = Some(first_child);
                    // Safety: We manipulate other child task inside the Mutex
                    // and retain a parent reference on it. The child token can't
                    // get invalidated while the Mutex is held.
                    unsafe {
                        first_child.as_mut().from_parent.prev_peer =
                            Some((&mut *child_token_state).into())
                    };
                }
                guard.first_child = Some((&mut *child_token_state).into());
            }
        };

        let child_token_ptr = Box::into_raw(child_token_state);
        // Safety: We just created the pointer from a `Box`
        CancellationToken {
            inner: unsafe { NonNull::new_unchecked(child_token_ptr) },
        }
    }

    /// Cancel the [`CancellationToken`] and all child tokens which had been
    /// derived from it.
    ///
    /// This will wake up all tasks which are waiting for cancellation.
    pub fn cancel(&self) {
        self.state().cancel();
    }

    /// Returns `true` if the `CancellationToken` had been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.state().is_cancelled()
    }

    /// Returns a `Future` that gets fulfilled when cancellation is requested.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        WaitForCancellationFuture {
            cancellation_token: Some(self),
            wait_node: ListNode::new(WaitQueueEntry::new()),
            is_registered: false,
        }
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
    }
}

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

        // So far the token is not cancelled. However it could be cancelld before
        // we get the chance to store the `Waker`. Therfore we need to check
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
                .map(|waker| waker.will_wake(cx.waker()))
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
