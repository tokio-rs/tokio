//! An asynchronously awaitable `CancellationToken`.
//! The token allows to signal a cancellation request to one or more tasks.
pub(crate) mod guard;

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
            if ::std::ptr::eq(actual_parent.as_ref(), potential_parent.as_ref()) {
                return (locked_node, Some(locked_parent));
            }
        }
    }
}
