use Error;
use timer::{Handle, Inner};

use futures::Poll;
use futures::task::AtomicTask;

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicUsize, AtomicBool, AtomicPtr};
use std::sync::atomic::Ordering::SeqCst;

#[derive(Debug)]
pub struct Entry {
    /// Timer internals
    inner: Weak<Inner>,

    /// Task to notify once the deadline is reached.
    task: AtomicTask,

    /// Tracks the entry state
    state: AtomicUsize,

    /// True wheen the entry is queued in the "process" linked list
    queued: AtomicBool,

    /// Next entry in the "process" linked list.
    ///
    /// Represents a strong Arc ref.
    next_atomic: UnsafeCell<*mut Entry>,

    /// When the entry expires, relative to the `start` of the timer
    /// (Inner::start).
    when: u64,

    /// Next entry in the State's linked list.
    ///
    /// This is only accessed by the timer
    next_stack: UnsafeCell<Option<Arc<Entry>>>,

    /// Previous entry in the State's linked list.
    ///
    /// This is only accessed by the timer and is used to unlink a canceled
    /// entry.
    ///
    /// This is a weak reference.
    prev_stack: UnsafeCell<*const Entry>,
}

/// A doubly linked stack
pub struct Stack {
    head: Option<Arc<Entry>>,
}

/// A stack of `Entry` nodes
#[derive(Debug)]
pub struct AtomicStack {
    /// Stack head
    head: AtomicPtr<Entry>,
}

/// Entries that were removed from the stack
#[derive(Debug)]
pub struct AtomicStackEntries {
    ptr: *mut Entry,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct State(usize);

/// Flag indicating a timer entry has elapsed
const ELAPSED: usize = 1;

/// Flag indicating a timer entry has reached an error state
const ERROR: usize = 1 << 2;

/// Used to indicate that the timer has shutdown.
const SHUTDOWN: *mut Entry = 1 as *mut _;

// ===== impl Entry =====

impl Entry {
    pub fn new(when: u64, handle: Handle) -> Entry {
        Entry {
            inner: handle.into_inner(),
            task: AtomicTask::new(),
            state: AtomicUsize::new(0),
            queued: AtomicBool::new(false),
            next_atomic: UnsafeCell::new(ptr::null_mut()),
            when,
            next_stack: UnsafeCell::new(None),
            prev_stack: UnsafeCell::new(ptr::null_mut()),
        }
    }

    /// Create a new `Entry` that is in the error state. Calling `poll_elapsed` on
    /// this `Entry` will always result in `Err` being returned.
    pub fn new_error() -> Entry {
        Entry {
            inner: Weak::new(),
            task: AtomicTask::new(),
            state: AtomicUsize::new(ELAPSED | ERROR),
            queued: AtomicBool::new(false),
            next_atomic: UnsafeCell::new(ptr::null_mut()),
            when: 0,
            next_stack: UnsafeCell::new(None),
            prev_stack: UnsafeCell::new(ptr::null_mut()),
        }
    }

    pub fn when(&self) -> u64 {
        self.when
    }

    pub fn is_elapsed(&self) -> bool {
        let state: State = self.state.load(SeqCst).into();
        state.is_elapsed()
    }

    pub fn fire(&self) {
        let state: State = self.state.fetch_or(ELAPSED, SeqCst).into();

        if state.is_elapsed() {
            return;
        }

        self.task.notify();
    }

    pub fn error(&self) {
        // Only transition to the error state if not currently elapsed
        let mut curr: State = self.state.load(SeqCst).into();

        loop {
            if curr.is_elapsed() {
                return;
            }

            let mut next = curr;
            next.set_error();

            let actual = self.state.compare_and_swap(
                curr.into(), next.into(), SeqCst).into();

            if curr == actual {
                break;
            }

            curr = actual;
        }

        self.task.notify();
    }

    pub fn cancel(entry: &Arc<Entry>) {
        let state: State = entry.state.fetch_or(ELAPSED, SeqCst).into();

        if state.is_elapsed() {
            // Nothing more to do
            return;
        }

        let inner = match entry.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        let _ = inner.queue(entry);
    }

    pub fn poll_elapsed(&self) -> Poll<(), Error> {
        use futures::Async::NotReady;

        let mut curr: State = self.state.load(SeqCst).into();

        if curr.is_elapsed() {
            if curr.is_error() {
                return Err(Error::shutdown());
            } else {
                return Ok(().into());
            }
        }

        self.task.register();

        curr = self.state.load(SeqCst).into();

        if curr.is_elapsed() {
            if curr.is_error() {
                return Err(Error::shutdown());
            } else {
                return Ok(().into());
            }
        }

        Ok(NotReady)
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        let inner = match self.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        inner.decrement();
    }
}

// ===== impl Stack =====

impl Stack {
    pub fn new() -> Stack {
        Stack { head: None }
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Push an entry to the head of the linked list
    pub fn push(&mut self, entry: Arc<Entry>) {
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

    /// Pop the head of the linked list
    pub fn pop(&mut self) -> Option<Arc<Entry>> {
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

    /// Remove the entry from the linked list
    ///
    /// The caller must ensure that the entry actually is contained by the list.
    pub fn remove(&mut self, entry: &Entry) {
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
}

// ===== impl AtomicStack =====

impl AtomicStack {
    pub fn new() -> AtomicStack {
        AtomicStack { head: AtomicPtr::new(ptr::null_mut()) }
    }

    /// Push an entry onto the stack.
    ///
    /// Returns `true` if the entry was pushed, `false` if the entry is already
    /// on the stack, `Err` if the timer is shutdown.
    pub fn push(&self, entry: &Arc<Entry>) -> Result<bool, Error> {
        // First, set the queued bit on the entry
        let queued = entry.queued.fetch_or(true, SeqCst).into();

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

    /// Take all entries from the stack
    pub fn take(&self) -> AtomicStackEntries {
        let ptr = self.head.swap(ptr::null_mut(), SeqCst);
        AtomicStackEntries { ptr }
    }

    /// Drain all remaining nodes in the stack and prevent any new nodes from
    /// being pushed onto the stack.
    pub fn shutdown(&self) {
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
        self.ptr = unsafe { (*entry.next_atomic.get()) };

        // Unset the queued flag
        let res = entry.queued.fetch_and(false, SeqCst);
        debug_assert!(res);

        // Return the entry
        Some(entry)
    }
}

impl Drop for AtomicStackEntries {
    fn drop(&mut self) {
        while let Some(entry) = self.next() {
            // Flag the entry as errored
            entry.error();
        }
    }
}

// ===== impl State =====

impl State {
    fn is_elapsed(&self) -> bool {
        self.0 & ELAPSED == ELAPSED
    }

    fn is_error(&self) -> bool {
        self.0 & ERROR == ERROR
    }

    fn set_error(&mut self) {
        self.0 |= ELAPSED | ERROR;
    }
}

impl From<usize> for State {
    fn from(src: usize) -> Self {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> Self {
        src.0
    }
}
