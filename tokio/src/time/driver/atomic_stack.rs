use crate::time::driver::Entry;
use crate::time::Error;

use std::ptr;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

/// A stack of `Entry` nodes
#[derive(Debug)]
pub(crate) struct AtomicStack {
    /// Stack head
    head: AtomicPtr<Entry>,
}

/// Entries that were removed from the stack
#[derive(Debug)]
pub(crate) struct AtomicStackEntries {
    ptr: *mut Entry,
}

/// Used to indicate that the timer has shutdown.
const SHUTDOWN: *mut Entry = 1 as *mut _;

impl AtomicStack {
    pub(crate) fn new() -> AtomicStack {
        AtomicStack {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Pushes an entry onto the stack.
    ///
    /// Returns `true` if the entry was pushed, `false` if the entry is already
    /// on the stack, `Err` if the timer is shutdown.
    pub(crate) fn push(&self, entry: &Arc<Entry>) -> Result<bool, Error> {
        // First, set the queued bit on the entry
        let queued = entry.queued.fetch_or(true, SeqCst);

        if queued {
            // Already queued, nothing more to do
            return Ok(false);
        }

        let ptr = Arc::into_raw(entry.clone()) as *mut _;

        let mut curr = self.head.load(SeqCst);

        loop {
            if curr == SHUTDOWN {
                // Don't leak the entry node
                let _ = unsafe { Arc::from_raw(ptr) };

                return Err(Error::shutdown());
            }

            // Update the `next` pointer. This is safe because setting the queued
            // bit is a "lock" on this field.
            unsafe {
                *(entry.next_atomic.get()) = curr;
            }

            let actual = self.head.compare_and_swap(curr, ptr, SeqCst);

            if actual == curr {
                break;
            }

            curr = actual;
        }

        Ok(true)
    }

    /// Takes all entries from the stack
    pub(crate) fn take(&self) -> AtomicStackEntries {
        let ptr = self.head.swap(ptr::null_mut(), SeqCst);
        AtomicStackEntries { ptr }
    }

    /// Drains all remaining nodes in the stack and prevent any new nodes from
    /// being pushed onto the stack.
    pub(crate) fn shutdown(&self) {
        // Shutdown the processing queue
        let ptr = self.head.swap(SHUTDOWN, SeqCst);

        // Let the drop fn of `AtomicStackEntries` handle draining the stack
        drop(AtomicStackEntries { ptr });
    }
}

// ===== impl AtomicStackEntries =====

impl Iterator for AtomicStackEntries {
    type Item = Arc<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr.is_null() {
            return None;
        }

        // Convert the pointer to an `Arc<Entry>`
        let entry = unsafe { Arc::from_raw(self.ptr) };

        // Update `self.ptr` to point to the next element of the stack
        self.ptr = unsafe { *entry.next_atomic.get() };

        // Unset the queued flag
        let res = entry.queued.fetch_and(false, SeqCst);
        debug_assert!(res);

        // Return the entry
        Some(entry)
    }
}

impl Drop for AtomicStackEntries {
    fn drop(&mut self) {
        for entry in self {
            // Flag the entry as errored
            entry.error();
        }
    }
}
