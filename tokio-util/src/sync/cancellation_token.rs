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
    use std::sync::{Arc, Mutex};

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
}
