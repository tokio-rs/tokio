#![cfg_attr(not(feature = "full"), allow(dead_code))]

//! An intrusive double linked list of data
//!
//! The data structure supports tracking pinned nodes. Most of the data
//! structure's APIs are `unsafe` as they require the caller to ensure the
//! specified node is actually contained by the list.

use core::fmt;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;

/// An intrusive linked list.
///
/// Currently, the list is not emptied on drop. It is the caller's
/// responsibility to ensure the list is empty before dropping it.
pub(crate) struct LinkedList<L, T> {
    /// Linked list head
    head: Option<NonNull<T>>,

    /// Linked list tail
    tail: Option<NonNull<T>>,

    /// Node type marker.
    _marker: PhantomData<*const L>,
}

unsafe impl<L: Link> Send for LinkedList<L, L::Target> where L::Target: Send {}
unsafe impl<L: Link> Sync for LinkedList<L, L::Target> where L::Target: Sync {}

/// Defines how a type is tracked within a linked list.
///
/// In order to support storing a single type within multiple lists, accessing
/// the list pointers is decoupled from the entry type.
///
/// # Safety
///
/// Implementations must guarantee that `Target` types are pinned in memory. In
/// other words, when a node is inserted, the value will not be moved as long as
/// it is stored in the list.
pub(crate) unsafe trait Link {
    /// Handle to the list entry.
    ///
    /// This is usually a pointer-ish type.
    type Handle;

    /// Node type
    type Target;

    /// Convert the handle to a raw pointer without consuming the handle
    fn as_raw(handle: &Self::Handle) -> NonNull<Self::Target>;

    /// Convert the raw pointer to a handle
    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle;

    /// Return the pointers for a node
    unsafe fn pointers(target: NonNull<Self::Target>) -> NonNull<Pointers<Self::Target>>;
}

/// Previous / next pointers
pub(crate) struct Pointers<T> {
    /// The previous node in the list. null if there is no previous node.
    prev: Option<NonNull<T>>,

    /// The next node in the list. null if there is no previous node.
    next: Option<NonNull<T>>,
}

unsafe impl<T: Send> Send for Pointers<T> {}
unsafe impl<T: Sync> Sync for Pointers<T> {}

// ===== impl LinkedList =====

impl<L, T> LinkedList<L, T> {
    /// Creates an empty linked list.
    pub(crate) const fn new() -> LinkedList<L, T> {
        LinkedList {
            head: None,
            tail: None,
            _marker: PhantomData,
        }
    }
}

impl<L: Link> LinkedList<L, L::Target> {
    /// Adds an element first in the list.
    pub(crate) fn push_front(&mut self, val: L::Handle) {
        // The value should not be dropped, it is being inserted into the list
        let val = ManuallyDrop::new(val);
        let ptr = L::as_raw(&*val);
        assert_ne!(self.head, Some(ptr));
        unsafe {
            L::pointers(ptr).as_mut().next = self.head;
            L::pointers(ptr).as_mut().prev = None;

            if let Some(head) = self.head {
                L::pointers(head).as_mut().prev = Some(ptr);
            }

            self.head = Some(ptr);

            if self.tail.is_none() {
                self.tail = Some(ptr);
            }
        }
    }

    /// Removes the last element from a list and returns it, or None if it is
    /// empty.
    pub(crate) fn pop_back(&mut self) -> Option<L::Handle> {
        unsafe {
            let last = self.tail?;
            self.tail = L::pointers(last).as_ref().prev;

            if let Some(prev) = L::pointers(last).as_ref().prev {
                L::pointers(prev).as_mut().next = None;
            } else {
                self.head = None
            }

            L::pointers(last).as_mut().prev = None;
            L::pointers(last).as_mut().next = None;

            Some(L::from_raw(last))
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

    /// Removes the specified node from the list
    ///
    /// # Safety
    ///
    /// The caller **must** ensure that `node` is currently contained by
    /// `self` or not contained by any other list.
    pub(crate) unsafe fn remove(&mut self, node: NonNull<L::Target>) -> Option<L::Handle> {
        if let Some(prev) = L::pointers(node).as_ref().prev {
            debug_assert_eq!(L::pointers(prev).as_ref().next, Some(node));
            L::pointers(prev).as_mut().next = L::pointers(node).as_ref().next;
        } else {
            if self.head != Some(node) {
                return None;
            }

            self.head = L::pointers(node).as_ref().next;
        }

        if let Some(next) = L::pointers(node).as_ref().next {
            debug_assert_eq!(L::pointers(next).as_ref().prev, Some(node));
            L::pointers(next).as_mut().prev = L::pointers(node).as_ref().prev;
        } else {
            // This might be the last item in the list
            if self.tail != Some(node) {
                return None;
            }

            self.tail = L::pointers(node).as_ref().prev;
        }

        L::pointers(node).as_mut().next = None;
        L::pointers(node).as_mut().prev = None;

        Some(L::from_raw(node))
    }
}

impl<L: Link> fmt::Debug for LinkedList<L, L::Target> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkedList")
            .field("head", &self.head)
            .field("tail", &self.tail)
            .finish()
    }
}

#[cfg(any(
    feature = "fs",
    all(unix, feature = "process"),
    feature = "signal",
    feature = "sync",
))]
impl<L: Link> LinkedList<L, L::Target> {
    pub(crate) fn last(&self) -> Option<&L::Target> {
        let tail = self.tail.as_ref()?;
        unsafe { Some(&*tail.as_ptr()) }
    }
}

impl<L: Link> Default for LinkedList<L, L::Target> {
    fn default() -> Self {
        Self::new()
    }
}

// ===== impl Iter =====

cfg_rt_threaded! {
    pub(crate) struct Iter<'a, T: Link> {
        curr: Option<NonNull<T::Target>>,
        _p: core::marker::PhantomData<&'a T>,
    }

    impl<L: Link> LinkedList<L, L::Target> {
        pub(crate) fn iter(&self) -> Iter<'_, L> {
            Iter {
                curr: self.head,
                _p: core::marker::PhantomData,
            }
        }
    }

    impl<'a, T: Link> Iterator for Iter<'a, T> {
        type Item = &'a T::Target;

        fn next(&mut self) -> Option<&'a T::Target> {
            let curr = self.curr?;
            // safety: the pointer references data contained by the list
            self.curr = unsafe { T::pointers(curr).as_ref() }.next;

            // safety: the value is still owned by the linked list.
            Some(unsafe { &*curr.as_ptr() })
        }
    }
}

// ===== impl DrainFilter =====

cfg_io_readiness! {
    pub(crate) struct DrainFilter<'a, T: Link, F> {
        list: &'a mut LinkedList<T, T::Target>,
        filter: F,
        curr: Option<NonNull<T::Target>>,
    }

    impl<T: Link> LinkedList<T, T::Target> {
        pub(crate) fn drain_filter<F>(&mut self, filter: F) -> DrainFilter<'_, T, F>
        where
            F: FnMut(&mut T::Target) -> bool,
        {
            let curr = self.head;
            DrainFilter {
                curr,
                filter,
                list: self,
            }
        }
    }

    impl<'a, T, F> Iterator for DrainFilter<'a, T, F>
    where
        T: Link,
        F: FnMut(&mut T::Target) -> bool,
    {
        type Item = T::Handle;

        fn next(&mut self) -> Option<Self::Item> {
            while let Some(curr) = self.curr {
                // safety: the pointer references data contained by the list
                self.curr = unsafe { T::pointers(curr).as_ref() }.next;

                // safety: the value is still owned by the linked list.
                if (self.filter)(unsafe { &mut *curr.as_ptr() }) {
                    return unsafe { self.list.remove(curr) };
                }
            }

            None
        }
    }
}

// ===== impl Pointers =====

impl<T> Pointers<T> {
    /// Create a new set of empty pointers
    pub(crate) fn new() -> Pointers<T> {
        Pointers {
            prev: None,
            next: None,
        }
    }
}

impl<T> fmt::Debug for Pointers<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pointers")
            .field("prev", &self.prev)
            .field("next", &self.next)
            .finish()
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use super::*;

    use std::pin::Pin;

    #[derive(Debug)]
    struct Entry {
        pointers: Pointers<Entry>,
        val: i32,
    }

    unsafe impl<'a> Link for &'a Entry {
        type Handle = Pin<&'a Entry>;
        type Target = Entry;

        fn as_raw(handle: &Pin<&'_ Entry>) -> NonNull<Entry> {
            NonNull::from(handle.get_ref())
        }

        unsafe fn from_raw(ptr: NonNull<Entry>) -> Pin<&'a Entry> {
            Pin::new(&*ptr.as_ptr())
        }

        unsafe fn pointers(mut target: NonNull<Entry>) -> NonNull<Pointers<Entry>> {
            NonNull::from(&mut target.as_mut().pointers)
        }
    }

    fn entry(val: i32) -> Pin<Box<Entry>> {
        Box::pin(Entry {
            pointers: Pointers::new(),
            val,
        })
    }

    fn ptr(r: &Pin<Box<Entry>>) -> NonNull<Entry> {
        r.as_ref().get_ref().into()
    }

    fn collect_list(list: &mut LinkedList<&'_ Entry, <&'_ Entry as Link>::Target>) -> Vec<i32> {
        let mut ret = vec![];

        while let Some(entry) = list.pop_back() {
            ret.push(entry.val);
        }

        ret
    }

    fn push_all<'a>(
        list: &mut LinkedList<&'a Entry, <&'_ Entry as Link>::Target>,
        entries: &[Pin<&'a Entry>],
    ) {
        for entry in entries.iter() {
            list.push_front(*entry);
        }
    }

    macro_rules! assert_clean {
        ($e:ident) => {{
            assert!($e.pointers.next.is_none());
            assert!($e.pointers.prev.is_none());
        }};
    }

    macro_rules! assert_ptr_eq {
        ($a:expr, $b:expr) => {{
            // Deal with mapping a Pin<&mut T> -> Option<NonNull<T>>
            assert_eq!(Some($a.as_ref().get_ref().into()), $b)
        }};
    }

    #[test]
    fn const_new() {
        const _: LinkedList<&Entry, <&Entry as Link>::Target> = LinkedList::new();
    }

    #[test]
    fn push_and_drain() {
        let a = entry(5);
        let b = entry(7);
        let c = entry(31);

        let mut list = LinkedList::new();
        assert!(list.is_empty());

        list.push_front(a.as_ref());
        assert!(!list.is_empty());
        list.push_front(b.as_ref());
        list.push_front(c.as_ref());

        let items: Vec<i32> = collect_list(&mut list);
        assert_eq!([5, 7, 31].to_vec(), items);

        assert!(list.is_empty());
    }

    #[test]
    fn push_pop_push_pop() {
        let a = entry(5);
        let b = entry(7);

        let mut list = LinkedList::<&Entry, <&Entry as Link>::Target>::new();

        list.push_front(a.as_ref());

        let entry = list.pop_back().unwrap();
        assert_eq!(5, entry.val);
        assert!(list.is_empty());

        list.push_front(b.as_ref());

        let entry = list.pop_back().unwrap();
        assert_eq!(7, entry.val);

        assert!(list.is_empty());
        assert!(list.pop_back().is_none());
    }

    #[test]
    fn remove_by_address() {
        let a = entry(5);
        let b = entry(7);
        let c = entry(31);

        unsafe {
            // Remove first
            let mut list = LinkedList::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);
            assert!(list.remove(ptr(&a)).is_some());
            assert_clean!(a);
            // `a` should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&a)).is_none());
            assert!(!list.is_empty());

            assert!(list.remove(ptr(&b)).is_some());
            assert_clean!(b);
            // `b` should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&b)).is_none());
            assert!(!list.is_empty());

            assert!(list.remove(ptr(&c)).is_some());
            assert_clean!(c);
            // `b` should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&c)).is_none());
            assert!(list.is_empty());
        }

        unsafe {
            // Remove middle
            let mut list = LinkedList::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&a)).is_some());
            assert_clean!(a);

            assert_ptr_eq!(b, list.head);
            assert_ptr_eq!(c, b.pointers.next);
            assert_ptr_eq!(b, c.pointers.prev);

            let items = collect_list(&mut list);
            assert_eq!([31, 7].to_vec(), items);
        }

        unsafe {
            // Remove middle
            let mut list = LinkedList::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&b)).is_some());
            assert_clean!(b);

            assert_ptr_eq!(c, a.pointers.next);
            assert_ptr_eq!(a, c.pointers.prev);

            let items = collect_list(&mut list);
            assert_eq!([31, 5].to_vec(), items);
        }

        unsafe {
            // Remove last
            // Remove middle
            let mut list = LinkedList::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&c)).is_some());
            assert_clean!(c);

            assert!(b.pointers.next.is_none());
            assert_ptr_eq!(b, list.tail);

            let items = collect_list(&mut list);
            assert_eq!([7, 5].to_vec(), items);
        }

        unsafe {
            // Remove first of two
            let mut list = LinkedList::new();

            push_all(&mut list, &[b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&a)).is_some());

            assert_clean!(a);

            // a should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&a)).is_none());

            assert_ptr_eq!(b, list.head);
            assert_ptr_eq!(b, list.tail);

            assert!(b.pointers.next.is_none());
            assert!(b.pointers.prev.is_none());

            let items = collect_list(&mut list);
            assert_eq!([7].to_vec(), items);
        }

        unsafe {
            // Remove last of two
            let mut list = LinkedList::new();

            push_all(&mut list, &[b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&b)).is_some());

            assert_clean!(b);

            assert_ptr_eq!(a, list.head);
            assert_ptr_eq!(a, list.tail);

            assert!(a.pointers.next.is_none());
            assert!(a.pointers.prev.is_none());

            let items = collect_list(&mut list);
            assert_eq!([5].to_vec(), items);
        }

        unsafe {
            // Remove last item
            let mut list = LinkedList::new();

            push_all(&mut list, &[a.as_ref()]);

            assert!(list.remove(ptr(&a)).is_some());
            assert_clean!(a);

            assert!(list.head.is_none());
            assert!(list.tail.is_none());
            let items = collect_list(&mut list);
            assert!(items.is_empty());
        }

        unsafe {
            // Remove missing
            let mut list = LinkedList::<&Entry, <&Entry as Link>::Target>::new();

            list.push_front(b.as_ref());
            list.push_front(a.as_ref());

            assert!(list.remove(ptr(&c)).is_none());
        }
    }

    #[test]
    fn iter() {
        let a = entry(5);
        let b = entry(7);

        let mut list = LinkedList::<&Entry, <&Entry as Link>::Target>::new();

        assert_eq!(0, list.iter().count());

        list.push_front(a.as_ref());
        list.push_front(b.as_ref());

        let mut i = list.iter();
        assert_eq!(7, i.next().unwrap().val);
        assert_eq!(5, i.next().unwrap().val);
        assert!(i.next().is_none());
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

        let mut ll = LinkedList::<&Entry, <&Entry as Link>::Target>::new();
        let mut reference = VecDeque::new();

        let entries: Vec<_> = (0..ops.len()).map(|i| entry(i as i32)).collect();

        for (i, op) in ops.iter().enumerate() {
            match op {
                Op::Push => {
                    reference.push_front(i as i32);
                    assert_eq!(entries[i].val, i as i32);

                    ll.push_front(entries[i].as_ref());
                }
                Op::Pop => {
                    if reference.is_empty() {
                        assert!(ll.is_empty());
                        continue;
                    }

                    let v = reference.pop_back();
                    assert_eq!(v, ll.pop_back().map(|v| v.val));
                }
                Op::Remove(n) => {
                    if reference.is_empty() {
                        assert!(ll.is_empty());
                        continue;
                    }

                    let idx = n % reference.len();
                    let expect = reference.remove(idx).unwrap();

                    unsafe {
                        let entry = ll.remove(ptr(&entries[expect as usize])).unwrap();
                        assert_eq!(expect, entry.val);
                    }
                }
            }
        }
    }
}
