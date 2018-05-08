use Error;
use atomic::AtomicU64;
use timer::{Handle, Inner};

use futures::Poll;
use futures::task::AtomicTask;

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicBool, AtomicPtr};
use std::sync::atomic::Ordering::SeqCst;
use std::time::Instant;
use std::u64;

/// Internal state shared between a `Delay` instance and the timer.
///
/// This struct is used as a node in two intrusive data structures:
///
/// * An atomic stack used to signal to the timer thread that the entry state
///   has changed. The timer thread will observe the entry on this stack and
///   perform any actions as necessary.
///
/// * A doubly linked list used **only** by the timer thread. Each slot in the
///   timer wheel is a head pointer to the list of entries that must be
///   processed during that timer tick.
#[derive(Debug)]
pub(crate) struct Entry {
    /// Timer internals. Using a weak pointer allows the timer to shutdown
    /// without all `Delay` instances having completed.
    inner: Weak<Inner>,

    /// Task to notify once the deadline is reached.
    task: AtomicTask,

    /// Tracks the entry state. This value contains the following information:
    ///
    /// * The deadline at which the entry must be "fired".
    /// * A flag indicating if the entry has already been fired.
    /// * Whether or not the entry transitioned to the error state.
    ///
    /// When an `Entry` is created, `state` is initialized to the instant at
    /// which the entry must be fired. When a timer is reset to a different
    /// instant, this value is changed.
    state: AtomicU64,

    /// When true, the entry is counted by `Inner` towards the max outstanding
    /// timeouts. The drop fn uses this to know if it should decrement the
    /// counter.
    ///
    /// One might think that it would be easier to just not create the `Entry`.
    /// The problem is that `Delay` expects creating a `Registration` to always
    /// return a `Registration` instance. This simplifying factor allows it to
    /// improve the struct layout. To do this, we must always allocate the node.
    counted: bool,

    /// True when the entry is queued in the "process" stack. This value
    /// is set before pushing the value and unset after popping the value.
    queued: AtomicBool,

    /// Next entry in the "process" linked list.
    ///
    /// Represents a strong Arc ref.
    next_atomic: UnsafeCell<*mut Entry>,

    /// When the entry expires, relative to the `start` of the timer
    /// (Inner::start). This is only used by the timer.
    ///
    /// A `Delay` instance can be reset to a different deadline by the thread
    /// that owns the `Delay` instance. In this case, the timer thread will not
    /// immediately know that this has happened. The timer thread must know the
    /// last deadline that it saw as it uses this value to locate the entry in
    /// its wheel.
    ///
    /// Once the timer thread observes that the instant has changed, it updates
    /// the wheel and sets this value. The idea is that this value eventually
    /// converges to the value of `state` as the timer thread makes updates.
    when: UnsafeCell<Option<u64>>,

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
pub(crate) struct Stack {
    head: Option<Arc<Entry>>,
}

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

/// Flag indicating a timer entry has elapsed
const ELAPSED: u64 = 1 << 63;

/// Flag indicating a timer entry has reached an error state
const ERROR: u64 = u64::MAX;

/// Used to indicate that the timer has shutdown.
const SHUTDOWN: *mut Entry = 1 as *mut _;

// ===== impl Entry =====

impl Entry {
    pub fn new(when: u64, handle: Handle) -> Entry {
        assert!(when > 0 && when < u64::MAX);

        Entry {
            inner: handle.into_inner(),
            task: AtomicTask::new(),
            state: AtomicU64::new(when),
            counted: true,
            queued: AtomicBool::new(false),
            next_atomic: UnsafeCell::new(ptr::null_mut()),
            when: UnsafeCell::new(None),
            next_stack: UnsafeCell::new(None),
            prev_stack: UnsafeCell::new(ptr::null_mut()),
        }
    }

    pub fn new_elapsed(handle: Handle) -> Entry {
        Entry {
            inner: handle.into_inner(),
            task: AtomicTask::new(),
            state: AtomicU64::new(ELAPSED),
            counted: true,
            queued: AtomicBool::new(false),
            next_atomic: UnsafeCell::new(ptr::null_mut()),
            when: UnsafeCell::new(None),
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
            state: AtomicU64::new(ERROR),
            counted: false,
            queued: AtomicBool::new(false),
            next_atomic: UnsafeCell::new(ptr::null_mut()),
            when: UnsafeCell::new(None),
            next_stack: UnsafeCell::new(None),
            prev_stack: UnsafeCell::new(ptr::null_mut()),
        }
    }

    /// The current entry state as known by the timer. This is not the value of
    /// `state`, but lets the timer know how to converge its state to `state`.
    pub fn when_internal(&self) -> Option<u64> {
        unsafe { (*self.when.get()) }
    }

    pub fn set_when_internal(&self, when: Option<u64>) {
        unsafe { (*self.when.get()) = when; }
    }

    /// Called by `Timer` to load the current value of `state` for processing
    pub fn load_state(&self) -> Option<u64> {
        let state = self.state.load(SeqCst);

        if is_elapsed(state) {
            None
        } else {
            Some(state)
        }
    }

    pub fn is_elapsed(&self) -> bool {
        let state = self.state.load(SeqCst);
        is_elapsed(state)
    }

    pub fn fire(&self, when: u64) {
        let mut curr = self.state.load(SeqCst);

        loop {
            if is_elapsed(curr) || curr > when {
                return;
            }

            let next = ELAPSED | curr;
            let actual = self.state.compare_and_swap(curr, next, SeqCst);

            if curr == actual {
                break;
            }

            curr = actual;
        }

        self.task.notify();
    }

    pub fn error(&self) {
        // Only transition to the error state if not currently elapsed
        let mut curr = self.state.load(SeqCst);

        loop {
            if is_elapsed(curr) {
                return;
            }

            let next = ERROR;

            let actual = self.state.compare_and_swap(curr, next, SeqCst);

            if curr == actual {
                break;
            }

            curr = actual;
        }

        self.task.notify();
    }

    pub fn cancel(entry: &Arc<Entry>) {
        let state = entry.state.fetch_or(ELAPSED, SeqCst);

        if is_elapsed(state) {
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

        let mut curr = self.state.load(SeqCst);

        if is_elapsed(curr) {
            if curr == ERROR {
                return Err(Error::shutdown());
            } else {
                return Ok(().into());
            }
        }

        self.task.register();

        curr = self.state.load(SeqCst).into();

        if is_elapsed(curr) {
            if curr == ERROR {
                return Err(Error::shutdown());
            } else {
                return Ok(().into());
            }
        }

        Ok(NotReady)
    }

    pub fn reset(entry: &Arc<Entry>, deadline: Instant) {
        let inner = match entry.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        let when = inner.normalize_deadline(deadline);
        let elapsed = inner.elapsed();

        let mut curr = entry.state.load(SeqCst);
        let mut notify;

        loop {
            // In these two cases, there is no work to do when resetting the
            // timer. If the `Entry` is in an error state, then it cannot be
            // used anymore. If resetting the entry to the current value, then
            // the reset is a noop.
            if curr == ERROR || curr == when {
                return;
            }

            let next;

            if when <= elapsed {
                next = ELAPSED;
                notify = !is_elapsed(curr);
            } else {
                next = when;
                notify = true;
            }

            let actual = entry.state.compare_and_swap(
                curr, next, SeqCst);

            if curr == actual {
                break;
            }

            curr = actual;
        }

        if notify {
            let _ = inner.queue(entry);
        }
    }
}

fn is_elapsed(state: u64) -> bool {
    state & ELAPSED == ELAPSED
}

impl Drop for Entry {
    fn drop(&mut self) {
        if !self.counted {
            return;
        }

        let inner = match self.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        inner.decrement();
    }
}

unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

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
