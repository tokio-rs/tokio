//! An asynchronously awaitable `CancellationToken`.
//! The token allows to signal a cancellation request to one or more tasks.
pub(crate) mod guard;

use crate::loom::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use guard::DropGuard;
use pin_project_lite::pin_project;

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

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationToken`]
    /// was cancelled
    #[must_use = "futures do nothing unless polled"]
    pub struct WaitForCancellationFuture<'a> {
        cancellation_token: &'a CancellationToken,
        #[pin]
        future: Option<tokio::sync::futures::Notified<'a>>,
    }
}

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
        CancellationToken {
            inner: self.inner.clone(),
        }
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
        CancellationToken {
            inner: implementation::child_node(&self.inner),
        }
    }

    /// Cancel the [`CancellationToken`] and all child tokens which had been
    /// derived from it.
    ///
    /// This will wake up all tasks which are waiting for cancellation.
    pub fn cancel(&self) {
        implementation::cancel(&self.inner);
    }

    /// Returns `true` if the `CancellationToken` had been cancelled
    pub fn is_cancelled(&self) -> bool {
        implementation::is_cancelled(&self.inner)
    }

    /// Returns a `Future` that gets fulfilled when cancellation is requested.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        WaitForCancellationFuture {
            cancellation_token: self,
            future: implementation::get_future(&self.inner),
        }
    }

    /// Creates a `DropGuard` for this token.
    ///
    /// Returned guard will cancel this token (and all its children) on drop
    /// unless disarmed.
    pub fn drop_guard(self) -> DropGuard {
        DropGuard { inner: Some(self) }
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
        let mut this = self.project();
        loop {
            {
                let future = match this.future.as_mut().as_pin_mut() {
                    Some(future) => future,
                    None => return Poll::Ready(()),
                };

                if future.poll(cx).is_pending() {
                    return Poll::Pending;
                }
            }

            // If the future was finished, try to query another one.
            // Once `get_future` returns `None`, we know the token was cancelled.
            this.future
                .set(implementation::get_future(&this.cancellation_token.inner));
        }
    }
}

/// GENERAL NOTES
/// -------------
///
/// ** Invariants **
///
/// Those invariants shall be true at any time, and are required to prove correctness
/// of the CancellationToken.
///
/// 1. A node that has no parents and no handles can no longer be cancelled.
///     This is important during both cancellation and refcounting.
///
/// 2. If node B *is* or *was* a child of node A, then node B was created *after* node A.
///     This is important for deadlock safety, as it is used for lock order.
///     Node B can only become the child of node A in two ways:
///         - being created with `child_node()`, in which case it is trivially true that
///           node A already existed when node B was created
///         - being moved A->C->B to A->B because node C was removed in `decrease_handle_refcount()`.
///           In this case the invariant still holds, as B was younger than C, and C was younger
///           than A, therefore B is also younger than A.
///
/// 3. If two nodes are both unlocked and node A is the parent of node B, then node B is a child of node A.
///     It is important to always restore that invariant before dropping the lock to a node.
///
/// ** Deadlock safety **
///
/// Principles that provide deadlock safety:
///     1. We always lock in the order of creation time. We can prove this through invariant #2.
///        Specifically, through invariant #2, we know that we always have to lock a parent before its child.
///     2. We never lock two siblings simultaneously, because we cannot establish an order.
///        There is one exception in `with_locked_node_and_parent()`, which is described in the function itself.
///
mod implementation {
    use crate::loom::sync::{Arc, Mutex, MutexGuard};

    /// A node of the cancellation tree structure
    ///
    /// The actual data it holds is wrapped inside a mutex for synchronization.
    pub(crate) struct TreeNode {
        inner: Mutex<Inner>,
        waker: tokio::sync::Notify,
    }
    impl TreeNode {
        pub(crate) fn new() -> Self {
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
    pub(crate) fn is_cancelled(node: &Arc<TreeNode>) -> bool {
        node.inner.lock().unwrap().is_cancelled
    }

    /// Creates a child node
    pub(crate) fn child_node(parent: &Arc<TreeNode>) -> Arc<TreeNode> {
        let mut locked_parent = parent.inner.lock().unwrap();

        // Do not register as child if we are already cancelled.
        // Cancelled trees can never be uncancelled and therefore
        // need no connection to parents or children any more.
        if locked_parent.is_cancelled {
            return Arc::new(TreeNode {
                inner: Mutex::new(Inner {
                    parent: None,
                    parent_idx: 0,
                    children: vec![],
                    is_cancelled: true,
                    num_handles: 1,
                }),
                waker: tokio::sync::Notify::new(),
            });
        }

        let child = Arc::new(TreeNode {
            inner: Mutex::new(Inner {
                parent: Some(parent.clone()),
                parent_idx: locked_parent.children.len(),
                children: vec![],
                is_cancelled: false,
                num_handles: 1,
            }),
            waker: tokio::sync::Notify::new(),
        });

        locked_parent.children.push(child.clone());

        child
    }

    /// Disconnects the given parent from all of its children.
    ///
    /// Takes a reference to [Inner] to make sure the parent is already locked.
    fn disconnect_children(node: &mut Inner) {
        for child in node.children.drain(..) {
            let mut locked_child = child.inner.lock().unwrap();
            locked_child.parent_idx = 0;
            locked_child.parent = None;
        }
    }

    /// Figures out the parent of the node and locks the node and its parent atomically.
    ///
    /// The basic principle of preventing deadlocks in the tree is
    /// that we always lock the parent first, and then the child.
    /// For more info look at *deadlock safety* and *invariant #2*.
    ///
    /// Sadly, it's impossible to figure out the parent of a node without
    /// locking it. To then achieve locking order consistency, the node
    /// has to be unlocked before the parent gets locked.
    /// This leaves a small window where we already assume that we know the parent,
    /// but neither the parent nor the node is locked. Therefore, the parent could change.
    ///
    /// To prevent that this problem leaks into the rest of the code, it is abstracted
    /// in this function.
    ///
    /// The locked child and optionally its locked parent, if a parent exists, get passed
    /// to the `func` argument via (node, None) or (node, Some(parent)).
    fn with_locked_node_and_parent<F, Ret>(node: &Arc<TreeNode>, func: F) -> Ret
    where
        F: FnOnce(MutexGuard<'_, Inner>, Option<MutexGuard<'_, Inner>>) -> Ret,
    {
        let mut potential_parent = {
            let locked_node = node.inner.lock().unwrap();
            match locked_node.parent.clone() {
                Some(parent) => parent,
                // If we locked the node and its parent is `None`, we are in a valid state
                // and can return.
                None => return func(locked_node, None),
            }
        };

        loop {
            // Deadlock safety:
            // At this very point, we *assume* that 'potential_parent' is the parent of 'node'.
            // But both are currently unlocked. Therefore, we cannot rely any more that this is the case.
            // Even worse, 'potential_parent' might actually become a *sibling* of 'node', which would violate
            // our 2. rule of deadlock safety.
            //
            // However, we can prove that the lock order has to be 'potential_parent' first, then 'node',
            // because based on invariant #2, 'node' is younger than 'potential_parent', as
            // 'potential_parent' once was the parent of 'node'.
            let locked_parent = potential_parent.inner.lock().unwrap();
            let locked_node = node.inner.lock().unwrap();

            let actual_parent = match locked_node.parent.clone() {
                Some(parent) => parent,
                // If we locked the node and its parent is `None`, we are in a valid state
                // and can return.
                None => return func(locked_node, None),
            };

            // Loop until we managed to lock both the node and its parent
            if Arc::ptr_eq(&actual_parent, &potential_parent) {
                return func(locked_node, Some(locked_parent));
            }

            drop(locked_node);
            drop(locked_parent);

            potential_parent = actual_parent;
        }
    }

    /// Moves all children from `node` to `parent`.
    ///
    /// `parent` MUST be the parent of `node`.
    /// To aquire the locks for node and parent, use [with_locked_node_and_parent].
    fn move_children_to_parent(node: &mut Inner, parent: &mut Inner) {
        // Pre-allocate in the parent, for performance
        parent.children.reserve(node.children.len());

        for child in node.children.drain(..) {
            {
                let mut child_locked = child.inner.lock().unwrap();
                child_locked.parent = node.parent.clone();
                child_locked.parent_idx = parent.children.len();
            }
            parent.children.push(child);
        }
    }

    /// Removes a child from the parent.
    ///
    /// `parent` MUST be the parent of `node`.
    /// To aquire the locks for node and parent, use [with_locked_node_and_parent].
    fn remove_child(parent: &mut Inner, mut node: MutexGuard<'_, Inner>) {
        // Query the position from where to remove a node
        let pos = node.parent_idx;
        node.parent = None;
        node.parent_idx = 0;

        // Unlock node, so that only one child at a time is locked.
        // Otherwise we would violate deadlock safety #2.
        drop(node);

        // If `node` is the last element in the list, we don't need any swapping
        if parent.children.len() == pos + 1 {
            parent.children.pop().unwrap();
        } else {
            // If `node` is not the last element in the list, we need to
            // replace it with the last element
            let replacement_child = parent.children.pop().unwrap();
            replacement_child.inner.lock().unwrap().parent_idx = pos;
            parent.children[pos] = replacement_child;
        }
    }

    /// Increases the reference count of handles.
    pub(crate) fn increase_handle_refcount(node: &Arc<TreeNode>) {
        let mut locked_node = node.inner.lock().unwrap();

        // Once no handles are left over, the node gets detached from the tree.
        // There should never be a new handle once all handles are dropped.
        assert!(locked_node.num_handles > 0);

        locked_node.num_handles += 1;
    }

    /// Decreses the reference count of handles.
    ///
    /// Once no handle is left, we can remove the node from the
    /// tree and connect its parent directly to its children.
    pub(crate) fn decrease_handle_refcount(node: &Arc<TreeNode>) {
        let num_handles = {
            let mut locked_node = node.inner.lock().unwrap();
            locked_node.num_handles -= 1;
            locked_node.num_handles
        };

        if num_handles == 0 {
            with_locked_node_and_parent(node, |mut node, parent| {
                // Move all children to parent
                match parent {
                    Some(mut parent) => {
                        // As we want to remove ourselves from the tree,
                        // we have to move the children to the parent, so that
                        // they still receive the cancellation event without us.
                        // Moving this does not violate invariant #1.
                        move_children_to_parent(&mut node, &mut parent);

                        // Remove the node from the parent
                        remove_child(&mut parent, node);
                    }
                    None => {
                        // Due to invariant #1, we can assume that our
                        // children can no longer be cancelled through us.
                        // (as we now have neither a parent nor handles)
                        // Therefore we can disconnect them.
                        disconnect_children(&mut node);
                    }
                }
            });
        }
    }

    /// Recursively cancels a subtree.
    ///
    /// Sadly, due to the constraint of keeping the parent's lock alive
    /// while iterating over the children (invariant #3) while NOT
    /// locking multiple children at the same time (deadlock prevention #2)
    /// makes it very hard to unroll this recursion.
    /// Feel free to try, but I gave up.
    fn cancel_locked_subtree(node: &mut Inner) {
        while let Some(child) = node.children.pop() {
            let mut locked_child = child.inner.lock().unwrap();

            locked_child.parent = None;
            locked_child.parent_idx = 0;

            locked_child.is_cancelled = true;
            child.waker.notify_waiters();

            // Propagate the cancellation to the child without dropping the lock,
            // as it is now disconnected from its parent. As soon as we drop the lock,
            // others (like [decrease_handle_refcount]) might assume that this is a node
            // without a parent, which can therefore no longer receive a cancellation. (invariant #1)
            cancel_locked_subtree(&mut locked_child);
        }
    }

    /// Cancels a node and its children.
    ///
    /// Then, dissolves the entire subtree for easier disposal.
    ///
    /// Invariant #1:
    /// It's important that during the entire process of cancellation, there is
    /// never a gap in lock propagation. We need to lock both the parent and the child for the
    /// entire duration of cancellation and disconnection. Those cannot happen separately,
    /// otherwise other calls like [decrease_handle_refcount] could modify the tree structure
    /// while we recurse over it.
    ///
    /// We basically have to create a "lock wave" from the parent to children, that nothing else
    /// can bypass. Otherwise, other actions could add/remove children that our cancellation would miss.
    ///
    /// Example: Holding a reference of a child does not influence its handle reference count, so
    /// it will happily drop its grandchildren in [decrease_handle_refcount] before we got the chance
    /// to propagate the cancellation to them, if we violate invariant #1.
    /// Unless we keep the child locked during the entire time, from the moment on when we
    /// disconnected it from its parent.
    pub(crate) fn cancel(node: &Arc<TreeNode>) {
        let waker = &node.waker;

        // Remove node from its parent, if parent exists
        with_locked_node_and_parent(node, |mut node, parent| {
            // Cancel the subtree
            node.is_cancelled = true;
            waker.notify_waiters();

            // Recursively cancel all children.
            cancel_locked_subtree(&mut node);

            // Disconnect the node from its parent.
            // Now that it is cancelled, it doesn't need any cancellation from
            // its parent any more.
            if let Some(mut parent) = parent {
                remove_child(&mut parent, node);
            };
        });
    }

    pub(crate) fn get_future(node: &Arc<TreeNode>) -> Option<tokio::sync::futures::Notified<'_>> {
        let notified = node.waker.notified();
        {
            let node_lock = node.inner.lock().unwrap();
            if node_lock.is_cancelled {
                return None;
            }
        }
        Some(notified)
    }
}
