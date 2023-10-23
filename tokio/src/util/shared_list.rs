#![cfg_attr(not(feature = "full"), allow(dead_code))]

use std::{ptr::NonNull, sync::atomic::Ordering};

use crate::loom::sync::{atomic::AtomicUsize, Mutex, MutexGuard};

use super::linked_list::{Link, LinkedList};

/// An intrusive  linked list supporting high concurrent updates.
///
/// It currently relies on `LinkedList`, so it is the caller's
/// responsibility to ensure the list is empty before dropping it.
///
/// Note: Due to its inner sharded design, the order of node cannot be guaranteed.
pub(crate) struct ShardedList<L, T> {
    lists: Box<[Mutex<LinkedList<L, T>>]>,
    count: AtomicUsize,
    shard_mask: usize,
}

/// Defines the id of a node in a linked list, different ids cause inner list nodes
/// to be scattered in different mutexs.
///
/// # Safety
///
/// Implementations must guarantee that `Target` types are pinned in memory. In
/// other words, when a node is inserted, the value will not be moved as long as
/// it is stored in the list.
pub(crate) unsafe trait ShardedListItem: Link {
    // The returned id is used to pick which list this item should go into.
    unsafe fn get_shared_id(target: NonNull<Self::Target>) -> usize;
}

impl<L, T> ShardedList<L, T> {
    /// Creates a new and empty sharded linked list under specified size
    pub(crate) fn new(sharded_size: usize) -> Self {
        // Find power-of-two sizes best matching arguments
        let mut size = 1;
        while size < sharded_size {
            size <<= 1;
        }
        let shard_mask = size - 1;

        let mut lists = Vec::with_capacity(size as usize);
        for _ in 0..size {
            lists.push(Mutex::new(LinkedList::<L, T>::new()))
        }
        Self {
            lists: lists.into_boxed_slice(),
            count: AtomicUsize::new(0),
            shard_mask,
        }
    }
}

impl<L: ShardedListItem> ShardedList<L, L::Target> {
    /// Adds an element into the list.
    pub(crate) fn push(&self, val: L::Handle) {
        // Safety: it is safe, because every ShardedListItem has an id for shard.
        let id = unsafe { L::get_shared_id(L::as_raw(&val)) };
        let mut lock = self.shard_inner(id);
        lock.push_front(val);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Removes the last element from a list specified by shard_id and returns it, or None if it is
    /// empty.
    pub(crate) fn pop_back(&self, shard_id: usize) -> Option<L::Handle> {
        let mut lock = self.shard_inner(shard_id);
        lock.pop_back()
    }

    /// Returns whether the linked list does not contain any node
    pub(crate) fn is_empty(&self) -> bool {
        self.count.load(Ordering::Relaxed) == 0
    }

    /// Removes the specified node from the list
    ///
    /// # Safety
    ///
    /// The caller **must** ensure that exactly one of the following is true:
    /// - `node` is currently contained by `self`,
    /// - `node` is not contained by any list,
    /// - `node` is currently contained by some other `GuardedLinkedList`.
    pub(crate) unsafe fn remove(&self, node: NonNull<L::Target>) -> Option<L::Handle> {
        let id = L::get_shared_id(node);
        let mut lock = self.shard_inner(id);
        let node = lock.remove(node);
        if node.is_some() {
            self.count.fetch_sub(1, Ordering::Relaxed);
        }
        node
    }

    /// Gets the count of elements in this list.
    pub(crate) fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Gets the shard size of this SharedList.
    /// Used to help us to decide the parameter `shard_id`` of the `pop_back` method.
    pub(crate) fn shard_size(&self) -> usize {
        self.shard_mask + 1
    }

    #[inline]
    fn shard_inner(&self, id: usize) -> MutexGuard<'_, LinkedList<L, <L as Link>::Target>> {
        // Safety: this modulo operation ensures it is safe here.
        unsafe { self.lists.get_unchecked(id & self.shard_mask).lock() }
    }
}

cfg_taskdump! {
    impl<L: ShardedListItem> ShardedList<L, L::Target> {
        pub(crate) fn for_each<F>(&self, f: &mut F)
        where
        F: FnMut(&L::Handle),
        {
            let mut guards = vec![];
            for list in self.lists.iter(){
                guards.push(list.lock());
            }
            for g in &mut guards{
                g.for_each(f);
            }
        }
    }
}
