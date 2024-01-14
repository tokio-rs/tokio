use super::{Link, LinkedList, LinkedListBase};

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use core::sync::atomic::{
    AtomicPtr,
    Ordering::{AcqRel, Relaxed},
};

/// An atomic intrusive linked list. It allows pushing new nodes concurrently.
/// Removing nodes still requires an exclusive reference.
pub(crate) struct AtomicLinkedList<L, T> {
    /// Linked list head.
    head: AtomicPtr<T>,

    /// Linked list tail.
    tail: UnsafeCell<Option<NonNull<T>>>,

    /// Node type marker.
    _marker: PhantomData<*const L>,
}

unsafe impl<L: Link> Send for AtomicLinkedList<L, L::Target> where L::Target: Send {}
unsafe impl<L: Link> Sync for AtomicLinkedList<L, L::Target> where L::Target: Sync {}

impl<L: Link> Default for AtomicLinkedList<L, L::Target> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L, T> AtomicLinkedList<L, T> {
    /// Creates an empty atomic linked list.
    pub(crate) const fn new() -> AtomicLinkedList<L, T> {
        AtomicLinkedList {
            head: AtomicPtr::new(core::ptr::null_mut()),
            tail: UnsafeCell::new(None),
            _marker: PhantomData,
        }
    }

    /// Convert an atomic linked list into a non-atomic version.
    pub(crate) fn into_list(mut self) -> LinkedList<L, T> {
        LinkedList {
            head: NonNull::new(*self.head.get_mut()),
            tail: *self.tail.get_mut(),
            _marker: PhantomData,
        }
    }
}

impl<L: Link> LinkedListBase<L> for AtomicLinkedList<L, L::Target> {
    fn head(&mut self) -> Option<NonNull<L::Target>> {
        NonNull::new(*self.head.get_mut())
    }

    fn tail(&mut self) -> Option<NonNull<L::Target>> {
        *self.tail.get_mut()
    }

    fn set_head(&mut self, node: Option<NonNull<L::Target>>) {
        *self.head.get_mut() = match node {
            Some(ptr) => ptr.as_ptr(),
            None => core::ptr::null_mut(),
        };
    }

    fn set_tail(&mut self, node: Option<NonNull<L::Target>>) {
        *self.tail.get_mut() = node;
    }
}

impl<L: Link> AtomicLinkedList<L, L::Target> {
    /// Atomically adds an element first in the list.
    /// This method can be called concurrently from multiple threads.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `val` is not pushed concurrently by muptiple threads,
    /// - `val` is not already part of some list.
    pub(crate) unsafe fn push_front(&self, val: L::Handle) {
        // Note that removing nodes from the list still requires
        // an exclusive reference, so we need not worry about
        // concurrent node removals.

        // The value should not be dropped, it is being inserted into the list.
        let val = ManuallyDrop::new(val);
        let new_head = L::as_raw(&val);

        // Safety: due to the function contract, no concurrent `push_front`
        // is called on this particular element, so we are safe to assume
        // ownership.
        L::pointers(new_head).as_mut().set_prev(None);

        let mut old_head = self.head.load(Relaxed);
        loop {
            // Safety: due to the function contract, no concurrent `push_front`
            // is called on this particular element, and we have not
            // inserted it into the list, so we can still assume ownership.
            L::pointers(new_head)
                .as_mut()
                .set_next(NonNull::new(old_head));

            if let Err(actual_head) =
                self.head
                    .compare_exchange_weak(old_head, new_head.as_ptr(), AcqRel, Relaxed)
            {
                old_head = actual_head;
            } else {
                break;
            };
        }

        if old_head.is_null() {
            // Safety: only the thread that successfully inserted the first
            // element is granted the right to update tail.
            *self.tail.get() = Some(new_head);
        } else {
            // Safety:
            // 1. Only the thread that successfully inserted the new element
            //    is granted the right to update the previous head's `prev`,
            // 2. Upon successfull insertion, we have synchronized with all the
            //    previous insertions (due to `AcqRel` memory ordering), so all
            //    the previous `set_prev` calls on `old_head` happen-before this call,
            // 3. Due the `push_front` contract, we can assume that `old_head`
            //    is not pushed concurrently by another thread, as it is already
            //    in the list. Thus, no data race on `set_prev` can happen.
            L::pointers(NonNull::new_unchecked(old_head))
                .as_mut()
                .set_prev(Some(new_head));
        }
    }

    /// See [LinkedList::remove].
    pub(crate) unsafe fn remove(&mut self, node: NonNull<L::Target>) -> Option<L::Handle> {
        LinkedListBase::remove(self, node)
    }
}

#[cfg(test)]
#[cfg(not(loom))]
#[cfg(not(target_os = "wasi"))] // Wasi doesn't support threads
pub(crate) mod tests {
    use super::super::tests::*;
    use super::*;

    use std::sync::Arc;

    #[test]
    fn atomic_push_front() {
        let atomic_list = Arc::new(AtomicLinkedList::<&Entry, <&Entry as Link>::Target>::new());

        let _entries = [5, 7]
            .into_iter()
            .map(|x| {
                std::thread::spawn({
                    let atomic_list = atomic_list.clone();
                    move || {
                        let list_entry = entry(x);
                        unsafe {
                            atomic_list.push_front(list_entry.as_ref());
                        }
                        list_entry
                    }
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        let mut list = Arc::into_inner(atomic_list).unwrap().into_list();

        assert!(!list.is_empty());

        let first = list.pop_back().unwrap();
        assert!(first.val == 5 || first.val == 7);

        let second = list.pop_back().unwrap();
        assert!(second.val == 5 || second.val == 7);
        assert_ne!(first.val, second.val);

        assert!(list.is_empty());
        assert!(list.pop_back().is_none());
    }
}
