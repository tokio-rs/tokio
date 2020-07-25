use crate::time::driver::Entry;
use crate::time::wheel;

use std::ptr;
use std::sync::Arc;

/// A doubly linked stack
#[derive(Debug)]
pub(crate) struct Stack {
    head: Option<Arc<Entry>>,
}

impl Default for Stack {
    fn default() -> Stack {
        Stack { head: None }
    }
}

impl wheel::Stack for Stack {
    type Owned = Arc<Entry>;
    type Borrowed = Entry;
    type Store = ();

    fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    fn push(&mut self, entry: Self::Owned, _: &mut Self::Store) {
        // Get a pointer to the entry to for the prev link
        let ptr: *const Entry = &*entry as *const _;

        // Remove the old head entry
        let old = self.head.take();

        unsafe {
            // Ensure the entry is not already in a stack.
            debug_assert!((*entry.next_stack.get()).is_none());
            debug_assert!((*entry.prev_stack.get()).is_null());

            if let Some(ref entry) = old.as_ref() {
                debug_assert!({
                    // The head is not already set to the entry
                    ptr != &***entry as *const _
                });

                // Set the previous link on the old head
                *entry.prev_stack.get() = ptr;
            }

            // Set this entry's next pointer
            *entry.next_stack.get() = old;
        }

        // Update the head pointer
        self.head = Some(entry);
    }

    /// Pops an item from the stack
    fn pop(&mut self, _: &mut ()) -> Option<Arc<Entry>> {
        let entry = self.head.take();

        unsafe {
            if let Some(entry) = entry.as_ref() {
                self.head = (*entry.next_stack.get()).take();

                if let Some(entry) = self.head.as_ref() {
                    *entry.prev_stack.get() = ptr::null();
                }

                *entry.prev_stack.get() = ptr::null();
            }
        }

        entry
    }

    fn remove(&mut self, entry: &Entry, _: &mut ()) {
        unsafe {
            // Ensure that the entry is in fact contained by the stack
            debug_assert!({
                // This walks the full linked list even if an entry is found.
                let mut next = self.head.as_ref();
                let mut contains = false;

                while let Some(n) = next {
                    if entry as *const _ == &**n as *const _ {
                        debug_assert!(!contains);
                        contains = true;
                    }

                    next = (*n.next_stack.get()).as_ref();
                }

                contains
            });

            // Unlink `entry` from the next node
            let next = (*entry.next_stack.get()).take();

            if let Some(next) = next.as_ref() {
                (*next.prev_stack.get()) = *entry.prev_stack.get();
            }

            // Unlink `entry` from the prev node

            if let Some(prev) = (*entry.prev_stack.get()).as_ref() {
                *prev.next_stack.get() = next;
            } else {
                // It is the head
                self.head = next;
            }

            // Unset the prev pointer
            *entry.prev_stack.get() = ptr::null();
        }
    }

    fn when(item: &Entry, _: &()) -> u64 {
        item.when_internal().expect("invalid internal state")
    }
}
