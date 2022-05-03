//! An asynchronously awaitable `CancellationToken`.
//! The token allows to signal a cancellation request to one or more tasks.
pub(crate) mod guard;

use crate::loom::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

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

/// A Future that is resolved once the corresponding [`CancellationToken`]
/// was cancelled
#[must_use = "futures do nothing unless polled"]
pub struct WaitForCancellationFuture<'a> {
    /// The CancellationToken that is associated with this WaitForCancellationFuture
    _cancellation_token: CancellationToken,
    /// Future, to wait for cancellation
    future: Option<Pin<Box<tokio::sync::futures::Notified<'a>>>>,
}

// Safety: Futures can be sent between threads as long as the underlying
// cancellation_token is thread-safe (Sync),
// which allows to poll/register/unregister from a different thread.
// TODO: figure out if this is still necessary
// unsafe impl<'a> Send for WaitForCancellationFuture<'a> {}

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
            _cancellation_token: self.clone(),
            future: implementation::get_future(&self.inner).map(Box::pin),
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

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match &mut self.future {
            Some(future) => Pin::new(future).poll(cx),
            None => Poll::Ready(()),
        }
    }
}

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
    ///
    /// Returns the (now parentless) children.
    fn disconnect_children(node: &mut Inner) -> Vec<Arc<TreeNode>> {
        for child in &node.children {
            let mut child = child.inner.lock().unwrap();
            child.parent_idx = 0;
            child.parent = None;
        }

        std::mem::take(&mut node.children)
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
            potential_parent = {
                // Deadlock safety:
                // This might deadlock if `node` suddenly became a PARENT of `potential_parent`.
                // This is impossible, though, as at most it might become a sibling or an uncle.
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

                actual_parent
            };
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
    fn remove_child(parent: &mut Inner, mut node: MutexGuard<'_, Inner>) -> Arc<TreeNode> {
        // Query the position from where to remove a node
        let pos = node.parent_idx;
        node.parent = None;
        node.parent_idx = 0;

        // Unlock node, so that only one child at a time is locked.
        // Locking both children simultaneously might cause a deadlock.
        drop(node);

        // If `node` is the last element in the list, we don't need any swapping
        if parent.children.len() == pos + 1 {
            return parent.children.pop().unwrap();
        }

        // If `node` is not the last element in the list, we need to swap it with the
        // last one before we can pop it.
        let replacement_child = parent.children.pop().unwrap();

        {
            let mut locked_replacement_child = replacement_child.inner.lock().unwrap();
            locked_replacement_child.parent_idx = pos;
        }

        // Swap the element to remove out of the list
        std::mem::replace(&mut parent.children[pos], replacement_child)
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
                        move_children_to_parent(&mut node, &mut parent);

                        // Remove the node from the parent
                        remove_child(&mut parent, node);
                    }
                    None => {
                        disconnect_children(&mut node);
                    }
                }
            });
        }
    }

    /// Cancels a node.
    ///
    /// Then, disconnects the node from the rest of the tree for easier disposal.
    pub(crate) fn cancel(node: &Arc<TreeNode>) {
        // Remove node from its parent, if parent exists
        with_locked_node_and_parent(node, |mut node, parent| {
            node.is_cancelled = true;
            match parent {
                Some(mut parent) => {
                    remove_child(&mut parent, node);
                }
                None => (),
            };
        });

        // Now `node` is a parent-less toplevel node.

        // Trigger waiting futures
        node.waker.notify_waiters();

        // Disconnect children
        let children = {
            let mut locked_node = node.inner.lock().unwrap();
            disconnect_children(&mut locked_node)
        };

        // Recursively cancel all children
        for child in children.into_iter() {
            cancel(&child);
        }
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
