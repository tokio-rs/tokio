//! An intrusive double linked list of data

use core::marker::PhantomPinned;
use core::ops::{Deref, DerefMut};
use core::ptr::null_mut;

/// A node which carries data of type `T` and is stored in an intrusive list
#[derive(Debug)]
pub(crate) struct ListNode<T> {
    /// The previous node in the list. null if there is no previous node.
    prev: *mut ListNode<T>,
    /// The next node in the list. null if there is no previous node.
    next: *mut ListNode<T>,
    /// The data which is associated to this list item
    data: T,
    /// Prevents `ListNode`s from being `Unpin`. They may never be moved, since
    /// the list semantics require addresses to be stable.
    _pin: PhantomPinned,
}

impl<T> ListNode<T> {
    /// Creates a new node with the associated data
    pub(crate) fn new(data: T) -> ListNode<T> {
        ListNode::<T> {
            prev: null_mut(),
            next: null_mut(),
            data,
            _pin: PhantomPinned,
        }
    }
}

impl<T> Deref for ListNode<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.data
    }
}

impl<T> DerefMut for ListNode<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

/// An intrusive linked list of nodes, where each node carries associated data
/// of type `T`.
#[derive(Debug)]
pub(crate) struct LinkedList<T> {
    head: *mut ListNode<T>,
    tail: *mut ListNode<T>,
}

impl<T> LinkedList<T> {
    /// Creates an empty linked list
    pub(crate) fn new() -> Self {
        LinkedList::<T> {
            head: null_mut(),
            tail: null_mut(),
        }
    }

    /// Consumes the list and creates an iterator over the linked list.
    /// This function is only safe as long as all pointers which are stored inside
    /// the linked list are valid.
    #[allow(dead_code)]
    pub(crate) unsafe fn into_iter(self) -> LinkedListIterator<T> {
        LinkedListIterator { current: self.head }
    }

    /// Consumes the list and creates an iterator over the linked list in reverse
    /// direction.
    /// This function is only safe as long as all pointers which are stored inside
    /// the linked list are valid.
    pub(crate) unsafe fn into_reverse_iter(self) -> LinkedListReverseIterator<T> {
        LinkedListReverseIterator { current: self.tail }
    }

    /// Adds an item to the front of the linked list.
    /// The function is only safe as long as valid pointers are stored inside
    /// the linked list.
    pub(crate) unsafe fn add_front(&mut self, item: *mut ListNode<T>) {
        assert!(!item.is_null(), "Can not add null pointers");
        (*item).next = self.head;
        (*item).prev = null_mut();
        if !self.head.is_null() {
            (*self.head).prev = item;
        }
        self.head = item;
        if self.tail.is_null() {
            self.tail = item;
        }
    }

    /// Returns the first item in the linked list without removing it from the list
    /// The function is only safe as long as valid pointers are stored inside
    /// the linked list.
    /// The returned pointer is only guaranteed to be valid as long as the list
    /// is not mutated
    #[allow(dead_code)]
    pub(crate) fn peek_first(&self) -> *mut ListNode<T> {
        self.head
    }

    /// Returns the last item in the linked list without removing it from the list
    /// The function is only safe as long as valid pointers are stored inside
    /// the linked list.
    /// The returned pointer is only guaranteed to be valid as long as the list
    /// is not mutated
    #[allow(dead_code)]
    pub(crate) fn peek_last(&self) -> *mut ListNode<T> {
        self.tail
    }

    /// Removes the last item from the linked list and returns it
    #[allow(dead_code)]
    pub(crate) unsafe fn remove_last(&mut self) -> *mut ListNode<T> {
        if self.tail.is_null() {
            return null_mut();
        }

        let last = self.tail;
        self.tail = (*last).prev;
        if !(*last).prev.is_null() {
            (*(*last).prev).next = null_mut();
        } else {
            // This was the last item in the list
            self.head = null_mut();
        }

        (*last).prev = null_mut();
        (*last).next = null_mut();
        last
    }

    /// Removes all items from the linked list and returns a LinkedList which
    /// contains all the items.
    pub(crate) fn take(&mut self) -> LinkedList<T> {
        let head = self.head;
        let tail = self.tail;
        self.head = null_mut();
        self.tail = null_mut();
        LinkedList::<T> { head, tail }
    }

    /// Returns whether the linked list doesn not contain any node
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        if !self.head.is_null() {
            return false;
        }

        assert!(self.tail.is_null());
        true
    }

    /// Removes the given item from the linked list.
    /// Returns whether the item was removed.
    /// The function is only safe as long as valid pointers are stored inside
    /// the linked list.
    pub(crate) unsafe fn remove(&mut self, item: *mut ListNode<T>) -> bool {
        if item.is_null() {
            return false;
        }

        let prev = (*item).prev;
        if prev.is_null() {
            // This might be the first item in the list
            if self.head != item {
                return false;
            }
            self.head = (*item).next;
        } else {
            debug_assert_eq!((*prev).next, item);
            (*prev).next = (*item).next;
        }

        let next = (*item).next;
        if next.is_null() {
            // This might be the last item in the list
            if self.tail != item {
                return false;
            }
            self.tail = (*item).prev;
        } else {
            debug_assert_eq!((*next).prev, item);
            (*next).prev = (*item).prev;
        }

        (*item).next = null_mut();
        (*item).prev = null_mut();

        true
    }
}

/// An iterator over an intrusively linked list
pub(crate) struct LinkedListIterator<T> {
    current: *mut ListNode<T>,
}

impl<T> Iterator for LinkedListIterator<T> {
    type Item = *mut ListNode<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }

        let node = self.current;
        // Safety: This is safe as long as the linked list is intact, which was
        // already required through the unsafe when creating the iterator.
        unsafe {
            self.current = (*self.current).next;
        }
        Some(node)
    }
}

/// An iterator over an intrusively linked list
pub(crate) struct LinkedListReverseIterator<T> {
    current: *mut ListNode<T>,
}

impl<T> Iterator for LinkedListReverseIterator<T> {
    type Item = *mut ListNode<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }

        let node = self.current;
        // Safety: This is safe as long as the linked list is intact, which was
        // already required through the unsafe when creating the iterator.
        unsafe {
            self.current = (*self.current).prev;
        }
        Some(node)
    }
}

#[cfg(test)]
#[cfg(feature = "std")] // Tests make use of Vec at the moment
mod tests {
    use super::*;

    unsafe fn collect_list<T: Copy>(list: LinkedList<T>) -> Vec<T> {
        list.into_iter().map(|item| (*(*item).deref())).collect()
    }

    unsafe fn collect_reverse_list<T: Copy>(list: LinkedList<T>) -> Vec<T> {
        list.into_reverse_iter()
            .map(|item| (*(*item).deref()))
            .collect()
    }

    unsafe fn add_nodes(list: &mut LinkedList<i32>, nodes: &mut [&mut ListNode<i32>]) {
        for node in nodes.iter_mut() {
            list.add_front(*node);
        }
    }

    unsafe fn assert_clean<T>(node: *mut ListNode<T>) {
        assert!((*node).next.is_null());
        assert!((*node).prev.is_null());
    }

    #[test]
    fn insert_and_iterate() {
        unsafe {
            let mut a = ListNode::new(5);
            let mut b = ListNode::new(7);
            let mut c = ListNode::new(31);

            let mut setup = |list: &mut LinkedList<i32>| {
                assert_eq!(true, list.is_empty());
                list.add_front(&mut c);
                assert_eq!(31, *(*list.peek_first()).deref());
                assert_eq!(false, list.is_empty());
                list.add_front(&mut b);
                assert_eq!(7, *(*list.peek_first()).deref());
                list.add_front(&mut a);
                assert_eq!(5, *(*list.peek_first()).deref());
            };

            let mut list = LinkedList::new();
            setup(&mut list);
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5, 7, 31].to_vec(), items);

            let mut list = LinkedList::new();
            setup(&mut list);
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([31, 7, 5].to_vec(), items);
        }
    }

    #[test]
    fn add_sorted() {
        unsafe {
            let mut a = ListNode::new(5);
            let mut b = ListNode::new(7);
            let mut c = ListNode::new(31);
            let mut d = ListNode::new(99);

            let mut list = LinkedList::new();
            list.add_sorted(&mut a);
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5].to_vec(), items);

            let mut list = LinkedList::new();
            list.add_sorted(&mut a);
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut d, &mut c, &mut b]);
            list.add_sorted(&mut a);
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5, 7, 31, 99].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut d, &mut c, &mut b]);
            list.add_sorted(&mut a);
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([99, 31, 7, 5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut d, &mut c, &mut a]);
            list.add_sorted(&mut b);
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5, 7, 31, 99].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut d, &mut c, &mut a]);
            list.add_sorted(&mut b);
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([99, 31, 7, 5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut d, &mut b, &mut a]);
            list.add_sorted(&mut c);
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5, 7, 31, 99].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut d, &mut b, &mut a]);
            list.add_sorted(&mut c);
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([99, 31, 7, 5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
            list.add_sorted(&mut d);
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5, 7, 31, 99].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
            list.add_sorted(&mut d);
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([99, 31, 7, 5].to_vec(), items);
        }
    }

    #[test]
    fn take_items() {
        unsafe {
            let mut a = ListNode::new(5);
            let mut b = ListNode::new(7);
            let mut c = ListNode::new(31);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);

            let taken = list.take();
            let items: Vec<i32> = collect_list(list);
            assert!(items.is_empty());
            let taken_items: Vec<i32> = collect_list(taken);
            assert_eq!([5, 7, 31].to_vec(), taken_items);
        }
    }

    #[test]
    fn peek_last() {
        unsafe {
            let mut a = ListNode::new(5);
            let mut b = ListNode::new(7);
            let mut c = ListNode::new(31);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);

            let last = list.peek_last();
            assert_eq!(31, *(*last).deref());
            list.remove_last();

            let last = list.peek_last();
            assert_eq!(7, *(*last).deref());
            list.remove_last();

            let last = list.peek_last();
            assert_eq!(5, *(*last).deref());
            list.remove_last();

            let last = list.peek_last();
            assert!(last.is_null());
        }
    }

    #[test]
    fn remove_last() {
        unsafe {
            let mut a = ListNode::new(5);
            let mut b = ListNode::new(7);
            let mut c = ListNode::new(31);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
            let removed = list.remove_last();
            assert_clean(removed);
            assert!(!list.is_empty());
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5, 7].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
            let removed = list.remove_last();
            assert_clean(removed);
            assert!(!list.is_empty());
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([7, 5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut b, &mut a]);
            let removed = list.remove_last();
            assert_clean(removed);
            assert!(!list.is_empty());
            let items: Vec<i32> = collect_list(list);
            assert_eq!([5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut b, &mut a]);
            let removed = list.remove_last();
            assert_clean(removed);
            assert!(!list.is_empty());
            let items: Vec<i32> = collect_reverse_list(list);
            assert_eq!([5].to_vec(), items);

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut a]);
            let removed = list.remove_last();
            assert_clean(removed);
            assert!(list.is_empty());
            let items: Vec<i32> = collect_list(list);
            assert!(items.is_empty());

            let mut list = LinkedList::new();
            add_nodes(&mut list, &mut [&mut a]);
            let removed = list.remove_last();
            assert_clean(removed);
            assert!(list.is_empty());
            let items: Vec<i32> = collect_reverse_list(list);
            assert!(items.is_empty());
        }
    }

    #[test]
    fn remove_by_address() {
        unsafe {
            let mut a = ListNode::new(5);
            let mut b = ListNode::new(7);
            let mut c = ListNode::new(31);

            {
                // Remove first
                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
                assert_eq!(true, list.remove(&mut a));
                assert_clean(&mut a);
                // a should be no longer there and can't be removed twice
                assert_eq!(false, list.remove(&mut a));
                assert_eq!(&mut b as *mut ListNode<i32>, list.head);
                assert_eq!(&mut c as *mut ListNode<i32>, b.next);
                assert_eq!(&mut b as *mut ListNode<i32>, c.prev);
                let items: Vec<i32> = collect_list(list);
                assert_eq!([7, 31].to_vec(), items);

                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
                assert_eq!(true, list.remove(&mut a));
                assert_clean(&mut a);
                // a should be no longer there and can't be removed twice
                assert_eq!(false, list.remove(&mut a));
                assert_eq!(&mut c as *mut ListNode<i32>, b.next);
                assert_eq!(&mut b as *mut ListNode<i32>, c.prev);
                let items: Vec<i32> = collect_reverse_list(list);
                assert_eq!([31, 7].to_vec(), items);
            }

            {
                // Remove middle
                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
                assert_eq!(true, list.remove(&mut b));
                assert_clean(&mut b);
                assert_eq!(&mut c as *mut ListNode<i32>, a.next);
                assert_eq!(&mut a as *mut ListNode<i32>, c.prev);
                let items: Vec<i32> = collect_list(list);
                assert_eq!([5, 31].to_vec(), items);

                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
                assert_eq!(true, list.remove(&mut b));
                assert_clean(&mut b);
                assert_eq!(&mut c as *mut ListNode<i32>, a.next);
                assert_eq!(&mut a as *mut ListNode<i32>, c.prev);
                let items: Vec<i32> = collect_reverse_list(list);
                assert_eq!([31, 5].to_vec(), items);
            }

            {
                // Remove last
                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
                assert_eq!(true, list.remove(&mut c));
                assert_clean(&mut c);
                assert!(b.next.is_null());
                assert_eq!(&mut b as *mut ListNode<i32>, list.tail);
                let items: Vec<i32> = collect_list(list);
                assert_eq!([5, 7].to_vec(), items);

                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut c, &mut b, &mut a]);
                assert_eq!(true, list.remove(&mut c));
                assert_clean(&mut c);
                assert!(b.next.is_null());
                assert_eq!(&mut b as *mut ListNode<i32>, list.tail);
                let items: Vec<i32> = collect_reverse_list(list);
                assert_eq!([7, 5].to_vec(), items);
            }

            {
                // Remove first of two
                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut b, &mut a]);
                assert_eq!(true, list.remove(&mut a));
                assert_clean(&mut a);
                // a should be no longer there and can't be removed twice
                assert_eq!(false, list.remove(&mut a));
                assert_eq!(&mut b as *mut ListNode<i32>, list.head);
                assert_eq!(&mut b as *mut ListNode<i32>, list.tail);
                assert!(b.next.is_null());
                assert!(b.prev.is_null());
                let items: Vec<i32> = collect_list(list);
                assert_eq!([7].to_vec(), items);

                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut b, &mut a]);
                assert_eq!(true, list.remove(&mut a));
                assert_clean(&mut a);
                // a should be no longer there and can't be removed twice
                assert_eq!(false, list.remove(&mut a));
                assert_eq!(&mut b as *mut ListNode<i32>, list.head);
                assert_eq!(&mut b as *mut ListNode<i32>, list.tail);
                assert!(b.next.is_null());
                assert!(b.prev.is_null());
                let items: Vec<i32> = collect_reverse_list(list);
                assert_eq!([7].to_vec(), items);
            }

            {
                // Remove last of two
                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut b, &mut a]);
                assert_eq!(true, list.remove(&mut b));
                assert_clean(&mut b);
                assert_eq!(&mut a as *mut ListNode<i32>, list.head);
                assert_eq!(&mut a as *mut ListNode<i32>, list.tail);
                assert!(a.next.is_null());
                assert!(a.prev.is_null());
                let items: Vec<i32> = collect_list(list);
                assert_eq!([5].to_vec(), items);

                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut b, &mut a]);
                assert_eq!(true, list.remove(&mut b));
                assert_clean(&mut b);
                assert_eq!(&mut a as *mut ListNode<i32>, list.head);
                assert_eq!(&mut a as *mut ListNode<i32>, list.tail);
                assert!(a.next.is_null());
                assert!(a.prev.is_null());
                let items: Vec<i32> = collect_reverse_list(list);
                assert_eq!([5].to_vec(), items);
            }

            {
                // Remove last item
                let mut list = LinkedList::new();
                add_nodes(&mut list, &mut [&mut a]);
                assert_eq!(true, list.remove(&mut a));
                assert_clean(&mut a);
                assert!(list.head.is_null());
                assert!(list.tail.is_null());
                let items: Vec<i32> = collect_list(list);
                assert!(items.is_empty());
            }

            {
                // Remove missing
                let mut list = LinkedList::new();
                list.add_front(&mut b);
                list.add_front(&mut a);
                assert_eq!(false, list.remove(&mut c));
            }

            {
                // Remove null
                let mut list = LinkedList::new();
                list.add_front(&mut b);
                list.add_front(&mut a);
                assert_eq!(false, list.remove(null_mut()));
            }
        }
    }
}
