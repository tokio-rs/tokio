//! This module defines an `IdleNotifiedSet` which is a collection of elements.
//! Each element is intended to correspond to a task, and the collection will
//! keep track of which tasks have notified their waker, and which have not.

use std::marker::PhantomPinned;
use std::ptr::NonNull;
use std::task::{Context, Waker};

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::{Arc, Mutex};
use crate::util::linked_list::{self, Link};
use crate::util::{waker_ref, Wake};

type LinkedList<T> =
    linked_list::LinkedList<ListEntry<T>, <ListEntry<T> as linked_list::Link>::Target>;

/// This is the main handle to the collection.
pub(crate) struct IdleNotifiedSet<T> {
    lists: Arc<Lists<T>>,
}

/// This is a handle to an element in the list. It can be converted into a
/// waker and the collection will keep track of whether that waker has been
/// notified.
pub(crate) struct IdleNotifiedEntry<T> {
    entry: Arc<ListEntry<T>>,
}

struct Lists<T> {
    inner: Mutex<ListsInner<T>>,
    /// Only accessed by `IdleNotifiedSet`.
    length: UnsafeCell<usize>,
}

/// The linked lists hold strong references to the ListEntry items, and the
/// ListEntry items also hold a strong reference back to the Lists object, but
/// the destructor of the `IdleNotifiedSet` will clear the two lists, so once
/// that object is destroyed, no ref-cycles will remain.
struct ListsInner<T> {
    notified: LinkedList<T>,
    idle: LinkedList<T>,
    waker: Option<Waker>,
}

/// Which of the two lists in the shared Lists object is this entry stored in?
///
/// If the value is `Idle`, then an entry's waker may move it to the notified
/// list. Otherwise, only the `IdleNotifiedSet` may move it.
///
/// If the value is `Neither`, then it is still possible that the entry is in
/// some third external list (this happens in `drain`).
#[derive(Copy, Clone, Eq, PartialEq)]
enum List {
    Notified,
    Idle,
    Neither,
}

/// An entry in the list.
///
/// The `my_list` and `pointers` fields must only be accessed while holding the
/// mutex. The `value` field must only be accessed by the `IdleNotifiedSet`.
///
/// However, if `my_list` is `Neither` and `pointers` still corresponds to some
/// list due to `drain` currently running, then `drain` may access the
/// `pointers` field without locking the mutex. However even in this case,
/// `my_list` still requires the mutex.
///
/// This struct has the invariant that the `my_list` field must store which
/// list, if any, this entry is stored in in its parent Lists.
#[repr(C)]
struct ListEntry<T> {
    /// The linked list pointers of the list this entry is in.
    pointers: linked_list::Pointers<ListEntry<T>>,
    /// Pointer to the shared `Lists` struct.
    parent: Arc<Lists<T>>,
    /// The value stored in this entry. The value is dropped when the item stops
    /// being a member of one of the two lists.
    value: UnsafeCell<T>,
    /// Used to remember which list this entry is in.
    my_list: UnsafeCell<List>,
    /// Required by the `linked_list::Pointers` field.
    _pin: PhantomPinned,
}

// With mutable access to the `IdleNotifiedSet`, you can get mutable access to
// the values.
unsafe impl<T: Send> Send for IdleNotifiedSet<T> {}
// With the current API we strictly speaking don't even need `T: Sync`, but we
// require it anyway to support adding &self APIs that access the values in the
// future.
unsafe impl<T: Sync> Sync for IdleNotifiedSet<T> {}

// These impls control when it is safe to create a Waker. Here, it is important
// that the value is Send because the value could get dropped on the thread
// containing the last Waker.
//
// It is not important for T to be Sync since the `value` cannot be accessed via
// the waker except through the destructor which takes `&mut self`.
unsafe impl<T: Send> Send for ListEntry<T> {}
unsafe impl<T: Send> Sync for ListEntry<T> {}

impl<T> IdleNotifiedSet<T> {
    /// Create a new IdleNotifiedSet.
    pub(crate) fn new() -> Self {
        let lists = Lists {
            inner: Mutex::new(ListsInner {
                notified: LinkedList::new(),
                idle: LinkedList::new(),
                waker: None,
            }),
            length: UnsafeCell::new(0),
        };

        IdleNotifiedSet {
            lists: Arc::new(lists),
        }
    }

    pub(crate) fn len(&self) -> usize {
        // Safety: We are the IdleNotifiedSet, so we may access the length.
        self.lists.length.with(|ptr| unsafe { *ptr })
    }

    /// Insert the given value into the `idle` list.
    pub(crate) fn insert_idle(&mut self, value: T) -> IdleNotifiedEntry<T> {
        // Safety: We are the IdleNotifiedSet, so we may access the length.
        self.lists.length.with_mut(|ptr| unsafe {
            *ptr += 1;
        });

        let entry = Arc::new(ListEntry {
            parent: self.lists.clone(),
            value: UnsafeCell::new(value),
            my_list: UnsafeCell::new(List::Idle),
            pointers: linked_list::Pointers::new(),
            _pin: PhantomPinned,
        });

        let entry = IdleNotifiedEntry::new(entry);

        let mut lock = self.lists.inner.lock();
        lock.idle.push_front(entry.clone());
        entry
    }

    /// Pop an entry from the notified list to poll it. The entry is moved to
    /// the idle list atomically.
    pub(crate) fn pop_notified(&mut self, waker: &Waker) -> Option<IdleNotifiedEntry<T>> {
        // Safety: We are the IdleNotifiedSet, so we may access the length.
        //
        // We don't decrement the length because this call moves the entry to
        // the idle list rather than removing it.
        let is_empty = self.lists.length.with(|ptr| unsafe { *ptr == 0 });
        if is_empty {
            // Fast path.
            return None;
        }

        let mut lock = self.lists.inner.lock();

        // Update the waker.
        if let Some(cur_waker) = lock.waker.as_mut() {
            if !waker.will_wake(cur_waker) {
                *cur_waker = waker.clone();
            }
        } else {
            lock.waker = Some(waker.clone());
        }

        // Pop the entry.
        match lock.notified.pop_back() {
            Some(entry) => {
                // Safety: We are holding the lock.
                lock.idle.push_front(entry.clone());
                entry.entry.my_list.with_mut(|ptr| unsafe {
                    *ptr = List::Idle;
                });
                drop(lock);
                Some(entry)
            }
            None => None,
        }
    }

    /// This removes the entry from the `idle` or `notified` list, depending on
    /// which list it is in.
    ///
    /// Returns whether it was actually removed.
    pub(crate) fn remove(&self, entry: &IdleNotifiedEntry<T>) -> bool {
        assert!(Arc::ptr_eq(&self.lists, &entry.entry.parent));

        let was_removed = {
            let mut lock = self.lists.inner.lock();

            // Safety: We are holding the lock so there is no race, and we will move
            // it between lists as appropriate afterwards to uphold invariants.
            let old_my_list = entry.entry.my_list.with_mut(|ptr| unsafe {
                let old_my_list = *ptr;
                *ptr = List::Neither;
                old_my_list
            });

            match old_my_list {
                List::Idle => {
                    unsafe {
                        // Safety: By checking that the parent pointers are the
                        // same, we have verified that this entry isn't in the
                        // idle list of some other set.
                        lock.idle.remove(ListEntry::as_raw(entry)).unwrap();
                    }
                    true
                }
                List::Notified => {
                    unsafe {
                        // Safety: By checking that the parent pointers are the
                        // same, we have verified that this entry isn't in the
                        // notified list of some other set.
                        lock.notified.remove(ListEntry::as_raw(entry)).unwrap();
                    }
                    true
                }
                List::Neither => false,
            }
        };

        if was_removed {
            // We are the IdleNotifiedSet, so we may access the length.
            self.lists.length.with_mut(|ptr| unsafe {
                *ptr -= 1;
            });
        }

        was_removed
    }

    /// Access the value in the provided entry.
    pub(crate) fn with_entry_value<F, U>(&mut self, entry: &IdleNotifiedEntry<T>, func: F) -> U
    where
        F: FnOnce(&mut T) -> U,
    {
        assert!(Arc::ptr_eq(&self.lists, &entry.entry.parent));

        // Safety: By checking the parent pointers for equality, we verify that
        // self really is the owner of the provided entry. It is therefore safe
        // to access the value.
        entry.entry.value.with_mut(|ptr| func(unsafe { &mut *ptr }))
    }

    /// Remove all entries in both lists, applying some function to each element.
    ///
    /// If the closure panics, it is not applied to the remaining elements, but the list is still
    /// cleared.
    pub(crate) fn drain<F: FnMut(&mut T)>(&mut self, mut func: F) {
        // We are the IdleNotifiedSet, so we may access the length.
        self.lists.length.with_mut(|ptr| unsafe {
            *ptr = 0;
        });

        // The LinkedList is not cleared on panic, so we use a bomb to clear it.
        struct AllEntries<T> {
            all_entries: LinkedList<T>,
        }

        impl<T> Drop for AllEntries<T> {
            fn drop(&mut self) {
                while let Some(entry) = self.all_entries.pop_back() {
                    drop(entry);
                }
            }
        }

        let mut all_entries = AllEntries {
            all_entries: LinkedList::new(),
        };

        {
            let mut lock = self.lists.inner.lock();
            unsafe {
                // Safety: We are holding the lock and `all_entries` is a new
                // LinkedList.
                move_to_new_list(&mut lock.idle, &mut all_entries.all_entries);
                move_to_new_list(&mut lock.notified, &mut all_entries.all_entries);
            }
        }

        // Safety: The entries have `my_list` set to `Neither`, so we can access
        // the `pointers` field.
        while let Some(entry) = all_entries.all_entries.pop_back() {
            // Safety: We are the IdleNotifiedSet so we can access the value.
            entry.entry.value.with_mut(|ptr| unsafe {
                func(&mut *ptr);
            });
        }
    }
}

impl<T> Drop for IdleNotifiedSet<T> {
    fn drop(&mut self) {
        let mut lock = self.lists.inner.lock();
        unsafe {
            // Safety: We are holding the lock.
            clear_list(&mut lock.idle);
            clear_list(&mut lock.notified);
        }
    }
}

/// The mutex for the entries must be held, and the target list must be such
/// that setting `my_list` to `Neither` is ok.
unsafe fn move_to_new_list<T>(from: &mut LinkedList<T>, to: &mut LinkedList<T>) {
    while let Some(entry) = from.pop_back() {
        entry.entry.my_list.with_mut(|ptr| unsafe {
            *ptr = List::Neither;
        });
        to.push_front(entry);
    }
}

/// The mutex for the entries must be held.
unsafe fn clear_list<T>(list: &mut LinkedList<T>) {
    while let Some(entry) = list.pop_back() {
        entry.entry.my_list.with_mut(|ptr| unsafe {
            *ptr = List::Neither;
        });
    }
}

impl<T: Send + 'static> Wake for ListEntry<T> {
    fn wake_by_ref(me: &Arc<Self>) {
        let mut lock = me.parent.inner.lock();

        // Safety: We are holding the lock and we will update the lists to
        // maintain invariants.
        let old_my_list = me.my_list.with_mut(|ptr| unsafe {
            let old_my_list = *ptr;
            if old_my_list == List::Idle {
                *ptr = List::Notified;
            }
            old_my_list
        });

        if old_my_list == List::Idle {
            // We move ourself to the notified list.
            let me = unsafe {
                // Safety: We just checked that we are in this particular list.
                lock.idle.remove(NonNull::from(&**me)).unwrap()
            };
            lock.notified.push_front(me);

            if let Some(waker) = lock.waker.take() {
                drop(lock);
                waker.wake();
            }
        }
    }

    fn wake(me: Arc<Self>) {
        Self::wake_by_ref(&me)
    }
}

/// # Safety
///
/// `ListEntry` is forced to be !Unpin.
unsafe impl<T> linked_list::Link for ListEntry<T> {
    type Handle = IdleNotifiedEntry<T>;
    type Target = ListEntry<T>;

    fn as_raw(handle: &Self::Handle) -> NonNull<ListEntry<T>> {
        let ptr: *const ListEntry<T> = Arc::as_ptr(&handle.entry);
        // Safety: We can't get a null pointer from `Arc::as_ptr`.
        unsafe { NonNull::new_unchecked(ptr as *mut ListEntry<T>) }
    }

    unsafe fn from_raw(ptr: NonNull<ListEntry<T>>) -> IdleNotifiedEntry<T> {
        IdleNotifiedEntry::new(Arc::from_raw(ptr.as_ptr()))
    }

    unsafe fn pointers(
        target: NonNull<ListEntry<T>>,
    ) -> NonNull<linked_list::Pointers<ListEntry<T>>> {
        // Safety: The pointers struct is the first field and ListEntry is
        // `#[repr(C)]` so this cast is safe.
        //
        // We do this rather than doing a field access since `std::ptr::addr_of`
        // is too new for our MSRV.
        target.cast()
    }
}

impl<T> IdleNotifiedEntry<T> {
    fn new(arc: Arc<ListEntry<T>>) -> Self {
        Self { entry: arc }
    }

    pub(crate) fn with_context<F, U>(&self, func: F) -> U
    where
        F: FnOnce(&mut Context<'_>) -> U,
        T: Send + 'static,
    {
        let waker = waker_ref(&self.entry);

        let mut context = Context::from_waker(&waker);
        func(&mut context)
    }
}

impl<T> Clone for IdleNotifiedEntry<T> {
    fn clone(&self) -> Self {
        Self {
            entry: self.entry.clone(),
        }
    }
}
