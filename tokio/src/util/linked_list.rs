//! An intrusive double linked list of data
//!
//! The data structure supports tracking pinned nodes. Most of the data
//! structure's APIs are `unsafe` as they require the caller to ensure the
//! specified node is actually contained by the list.

use core::cell::UnsafeCell;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::ptr::NonNull;

/// An intrusive linked list of nodes, where each node carries associated data
/// of type `T`.
#[derive(Debug)]
pub(crate) struct LinkedList<T> {
    head: Option<NonNull<Entry<T>>>,
    tail: Option<NonNull<Entry<T>>>,
}

unsafe impl<T: Send> Send for LinkedList<T> {}
unsafe impl<T: Sync> Sync for LinkedList<T> {}

/// A node which carries data of type `T` and is stored in an intrusive list.
#[derive(Debug)]
pub(crate) struct Entry<T> {
    /// The previous node in the list. null if there is no previous node.
    prev: Option<NonNull<Entry<T>>>,

    /// The next node in the list. null if there is no previous node.
    next: Option<NonNull<Entry<T>>>,

    /// The data which is associated to this list item
    data: UnsafeCell<T>,

    /// Prevents `Entry`s from being `Unpin`. They may never be moved, since
    /// the list semantics require addresses to be stable.
    _pin: PhantomPinned,
}

unsafe impl<T: Send> Send for Entry<T> {}
unsafe impl<T: Sync> Sync for Entry<T> {}

impl<T> LinkedList<T> {
    /// Creates an empty linked list
    pub(crate) fn new() -> Self {
        LinkedList {
            head: None,
            tail: None,
        }
    }

    /// Adds an item to the back of the linked list.
    ///
    /// # Safety
    ///
    /// The function is only safe as long as valid pointers are stored inside
    /// the linked list.
    pub(crate) unsafe fn push_front(&mut self, entry: Pin<&mut Entry<T>>) {
        let mut entry: NonNull<Entry<T>> = entry.get_unchecked_mut().into();

        entry.as_mut().next = self.head;
        entry.as_mut().prev = None;

        if let Some(head) = &mut self.head {
            head.as_mut().prev = Some(entry);
        }

        self.head = Some(entry);

        if self.tail.is_none() {
            self.tail = Some(entry);
        }
    }

    /// Removes the first element and returns it, or `None` if the list is empty.
    ///
    /// The function is safe as the lifetime of the entry is bound to `&mut
    /// self`.
    pub(crate) fn pop_back(&mut self) -> Option<Pin<&mut T>> {
        unsafe {
            let mut last = self.tail?;
            self.tail = last.as_ref().prev;

            if let Some(mut prev) = last.as_mut().prev {
                prev.as_mut().next = None;
            } else {
                self.head = None
            }

            last.as_mut().prev = None;
            last.as_mut().next = None;

            let val = &mut *last.as_mut().data.get();

            Some(Pin::new_unchecked(val))
        }
    }

    /// Returns whether the linked list doesn not contain any node
    pub(crate) fn is_empty(&self) -> bool {
        if self.head.is_some() {
            return false;
        }

        assert!(self.tail.is_none());
        true
    }

    /// Removes the given item from the linked list.
    ///
    /// # Safety
    ///
    /// The caller **must** ensure that `entry` is currently contained by
    /// `self`.
    pub(crate) unsafe fn remove(&mut self, entry: Pin<&mut Entry<T>>) -> bool {
        let mut entry = NonNull::from(entry.get_unchecked_mut());

        if let Some(mut prev) = entry.as_mut().prev {
            debug_assert_eq!(prev.as_ref().next, Some(entry));
            prev.as_mut().next = entry.as_ref().next;
        } else {
            if self.head != Some(entry) {
                return false;
            }

            self.head = entry.as_ref().next;
        }

        if let Some(mut next) = entry.as_mut().next {
            debug_assert_eq!(next.as_ref().prev, Some(entry));
            next.as_mut().prev = entry.as_ref().prev;
        } else {
            // This might be the last item in the list
            if self.tail != Some(entry) {
                return false;
            }

            self.tail = entry.as_ref().prev;
        }

        entry.as_mut().next = None;
        entry.as_mut().prev = None;

        true
    }
}

impl<T> Entry<T> {
    /// Creates a new node with the associated data
    pub(crate) fn new(data: T) -> Entry<T> {
        Entry {
            prev: None,
            next: None,
            data: UnsafeCell::new(data),
            _pin: PhantomPinned,
        }
    }

    /// Get a raw pointer to the inner data
    pub(crate) fn get(&self) -> *mut T {
        self.data.get()
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use super::*;

    fn collect_list<T: Copy>(list: &mut LinkedList<T>) -> Vec<T> {
        let mut ret = vec![];

        while let Some(v) = list.pop_back() {
            ret.push(*v);
        }

        ret
    }

    unsafe fn push_all(list: &mut LinkedList<i32>, entries: &mut [Pin<&mut Entry<i32>>]) {
        for entry in entries.iter_mut() {
            list.push_front(entry.as_mut());
        }
    }

    macro_rules! assert_clean {
        ($e:ident) => {{
            assert!($e.next.is_none());
            assert!($e.prev.is_none());
        }};
    }

    macro_rules! assert_ptr_eq {
        ($a:expr, $b:expr) => {{
            // Deal with mapping a Pin<&mut T> -> Option<NonNull<T>>
            assert_eq!(Some($a.as_mut().get_unchecked_mut().into()), $b)
        }};
    }

    #[test]
    fn push_and_drain() {
        pin! {
            let a = Entry::new(5);
            let b = Entry::new(7);
            let c = Entry::new(31);
        }

        let mut list = LinkedList::new();
        assert!(list.is_empty());

        unsafe {
            list.push_front(a);
            assert!(!list.is_empty());
            list.push_front(b);
            list.push_front(c);
        }

        let items: Vec<i32> = collect_list(&mut list);
        assert_eq!([5, 7, 31].to_vec(), items);

        assert!(list.is_empty());
    }

    #[test]
    fn push_pop_push_pop() {
        pin! {
            let a = Entry::new(5);
            let b = Entry::new(7);
        }

        let mut list = LinkedList::new();

        unsafe {
            list.push_front(a);
        }

        let v = list.pop_back().unwrap();
        assert_eq!(5, *v);
        assert!(list.is_empty());

        unsafe {
            list.push_front(b);
        }

        let v = list.pop_back().unwrap();
        assert_eq!(7, *v);

        assert!(list.is_empty());
        assert!(list.pop_back().is_none());
    }

    #[test]
    fn remove_by_address() {
        pin! {
            let a = Entry::new(5);
            let b = Entry::new(7);
            let c = Entry::new(31);
        }

        unsafe {
            // Remove first
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [c.as_mut(), b.as_mut(), a.as_mut()]);
            assert!(list.remove(a.as_mut()));
            assert_clean!(a);
            // `a` should be no longer there and can't be removed twice
            assert!(!list.remove(a.as_mut()));
            assert!(!list.is_empty());

            assert!(list.remove(b.as_mut()));
            assert_clean!(b);
            // `b` should be no longer there and can't be removed twice
            assert!(!list.remove(b.as_mut()));
            assert!(!list.is_empty());

            assert!(list.remove(c.as_mut()));
            assert_clean!(c);
            // `b` should be no longer there and can't be removed twice
            assert!(!list.remove(c.as_mut()));
            assert!(list.is_empty());
        }

        unsafe {
            // Remove middle
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [c.as_mut(), b.as_mut(), a.as_mut()]);

            assert!(list.remove(a.as_mut()));
            assert_clean!(a);

            assert_ptr_eq!(b, list.head);
            assert_ptr_eq!(c, b.next);
            assert_ptr_eq!(b, c.prev);

            let items = collect_list(&mut list);
            assert_eq!([31, 7].to_vec(), items);
        }

        unsafe {
            // Remove middle
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [c.as_mut(), b.as_mut(), a.as_mut()]);

            assert!(list.remove(b.as_mut()));
            assert_clean!(b);

            assert_ptr_eq!(c, a.next);
            assert_ptr_eq!(a, c.prev);

            let items = collect_list(&mut list);
            assert_eq!([31, 5].to_vec(), items);
        }

        unsafe {
            // Remove last
            // Remove middle
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [c.as_mut(), b.as_mut(), a.as_mut()]);

            assert!(list.remove(c.as_mut()));
            assert_clean!(c);

            assert!(b.next.is_none());
            assert_ptr_eq!(b, list.tail);

            let items = collect_list(&mut list);
            assert_eq!([7, 5].to_vec(), items);
        }

        unsafe {
            // Remove first of two
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [b.as_mut(), a.as_mut()]);

            assert!(list.remove(a.as_mut()));

            assert_clean!(a);

            // a should be no longer there and can't be removed twice
            assert!(!list.remove(a.as_mut()));

            assert_ptr_eq!(b, list.head);
            assert_ptr_eq!(b, list.tail);

            assert!(b.next.is_none());
            assert!(b.prev.is_none());

            let items = collect_list(&mut list);
            assert_eq!([7].to_vec(), items);
        }

        unsafe {
            // Remove last of two
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [b.as_mut(), a.as_mut()]);

            assert!(list.remove(b.as_mut()));

            assert_clean!(b);

            assert_ptr_eq!(a, list.head);
            assert_ptr_eq!(a, list.tail);

            assert!(a.next.is_none());
            assert!(a.prev.is_none());

            let items = collect_list(&mut list);
            assert_eq!([5].to_vec(), items);
        }

        unsafe {
            // Remove last item
            let mut list = LinkedList::new();

            push_all(&mut list, &mut [a.as_mut()]);

            assert!(list.remove(a.as_mut()));
            assert_clean!(a);

            assert!(list.head.is_none());
            assert!(list.tail.is_none());
            let items = collect_list(&mut list);
            assert!(items.is_empty());
        }

        unsafe {
            // Remove missing
            let mut list = LinkedList::new();

            list.push_front(b.as_mut());
            list.push_front(a.as_mut());

            assert!(!list.remove(c.as_mut()));
        }
    }

    proptest::proptest! {
        #[test]
        fn fuzz_linked_list(ops: Vec<usize>) {
            run_fuzz(ops);
        }
    }

    fn run_fuzz(ops: Vec<usize>) {
        use std::collections::VecDeque;

        #[derive(Debug)]
        enum Op {
            Push,
            Pop,
            Remove(usize),
        }

        let ops = ops
            .iter()
            .map(|i| match i % 3 {
                0 => Op::Push,
                1 => Op::Pop,
                2 => Op::Remove(i / 3),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();

        let mut next = 0;
        let mut ll = LinkedList::new();
        let mut entries = VecDeque::new();
        let mut reference = VecDeque::new();

        for op in ops {
            match op {
                Op::Push => {
                    let v = next;
                    next += 1;

                    reference.push_front(v);
                    entries.push_front(Box::pin(Entry::new(v)));

                    unsafe {
                        ll.push_front(entries.front_mut().unwrap().as_mut());
                    }
                }
                Op::Pop => {
                    if reference.is_empty() {
                        assert!(ll.is_empty());
                        continue;
                    }

                    let v = reference.pop_back();
                    assert_eq!(v, ll.pop_back().map(|v| *v));
                    entries.pop_back();
                }
                Op::Remove(n) => {
                    if reference.is_empty() {
                        assert!(ll.is_empty());
                        continue;
                    }

                    let idx = n % reference.len();

                    unsafe {
                        assert!(ll.remove(entries[idx].as_mut()));
                    }

                    let v = reference.remove(idx).unwrap();
                    assert_eq!(v, unsafe { *entries[idx].get() });

                    entries.remove(idx);
                }
            }
        }
    }
}
